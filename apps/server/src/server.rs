//! Binding, routing and serving.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;

use axum::middleware;
use axum::routing::{any, get, post, MethodRouter};
use axum::Router;

use crate::api::{self, Api};
use crate::auth::{self, Gate};
use crate::guard::{self, Hosts};
use crate::log::{self, Log, Sink};
use crate::panel::{self, Panel};
use crate::token::Token;
use crate::transcript::Transcript;
use crate::writer::Writer;
use crate::Backend;

/// A server not yet listening: a backend, and how to serve it.
pub struct Server<B: Backend> {
    backend: B,
    panel: Panel,
    log: Option<Sink>,
    port: u16,
}

impl<B: Backend> Server<B> {
    /// A server for the panel built into this binary, that logs nothing.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            panel: Panel::default(),
            log: None,
            port: 0,
        }
    }

    /// Serve these files as the panel instead.
    pub fn panel(mut self, panel: Panel) -> Self {
        self.panel = panel;
        self
    }

    /// Hand every request's line to `sink` once it has been answered, with
    /// the launch token redacted wherever it appears.
    pub fn log(mut self, sink: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.log = Some(Arc::new(sink));
        self
    }

    /// Listen on `port` rather than on one the system picks; 0 still asks
    /// for any.
    ///
    /// For a client that must know the port before the server starts, as
    /// the panel's dev server does, whose proxy names it in its settings.
    /// A port somebody else holds makes [`bind`](Self::bind) fail rather
    /// than take another, where that client would never find it.
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Take a port and issue the launch's token.
    ///
    /// 127.0.0.1 and nothing else, not even as an option: the panel is for
    /// the person at this machine, and every address beyond loopback is a
    /// network where the token would travel in clear text. Port 0 unless the
    /// front end named one ([`port`](Self::port)), so the system picks one
    /// that is free rather than the server failing on a port somebody else
    /// took.
    ///
    /// Synchronous, and std's listener rather than tokio's, so that a front
    /// end can learn the port and print the link before it has a runtime.
    pub fn bind(self) -> io::Result<Listening> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, self.port))?;
        let address = listener.local_addr()?;
        let token = Token::issue();
        let router = route(self, &token, address.port())?;
        Ok(Listening {
            listener,
            address,
            token,
            router,
        })
    }
}

/// A server with its port and its token, ready to serve.
pub struct Listening {
    listener: TcpListener,
    address: SocketAddr,
    token: Token,
    router: Router,
}

impl Listening {
    /// Where it listens: 127.0.0.1, on the port the system chose.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The launch's token, for a front end that talks to the server itself.
    /// A browser gets it through [`Listening::url`] instead.
    pub fn token(&self) -> &str {
        self.token.as_str()
    }

    /// The link that opens the panel: `http://127.0.0.1:<port>/?token=...`.
    /// The first request trades the token for a cookie and drops it from the
    /// address bar.
    pub fn url(&self) -> String {
        format!(
            "http://{}/?{}={}",
            self.address,
            auth::PARAMETER,
            self.token.as_str()
        )
    }

    /// The name of the cookie the token is kept in on this port.
    pub fn cookie(&self) -> String {
        auth::cookie_name(self.address.port())
    }

    /// Every route, for a caller that wants to send requests in process
    /// rather than over the socket. Each still needs the Host header one over
    /// the socket would carry, naming this server.
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// Answer requests until `shutdown` completes.
    pub async fn serve(
        self,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> io::Result<()> {
        self.listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(self.listener)?;
        axum::serve(listener, self.router)
            .with_graceful_shutdown(shutdown)
            .await
    }
}

/// Every route, behind what guards it.
fn route<B: Backend>(server: Server<B>, token: &Token, port: u16) -> io::Result<Router> {
    let gate = Arc::new(Gate::new(token.clone(), port));
    let log = server
        .log
        .map(|sink| Arc::new(Log::new(sink, token.clone())));
    let backend = Arc::new(server.backend);
    let transcript = Transcript::default();
    let api = Arc::new(Api {
        writer: Writer::start(Arc::clone(&backend), transcript.clone())?,
        backend,
        transcript,
    });
    let router = Router::new()
        .route("/api/query", endpoint(post(api::query::<B>)))
        .route("/api/commands/preview", endpoint(post(api::preview::<B>)))
        .route("/api/commands", endpoint(post(api::commands::<B>)))
        .route("/api/transcript", endpoint(get(api::transcript::<B>)))
        // The bare prefix and everything under it that names no endpoint,
        // so a typo in a script gets a JSON 404 rather than the panel. `/api/`
        // needs a route of its own: the catch-all wants at least one
        // character after the slash, and without one the prefix would reach
        // the panel past the gate.
        .route("/api", any(api::missing))
        .route("/api/", any(api::missing))
        .route("/api/{*rest}", any(api::missing))
        // `route_layer`, so the gate guards the routes above and nothing
        // added after them: not the panel, whose files are the same for
        // everybody.
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&gate),
            auth::require,
        ))
        .with_state(api)
        .fallback(panel::serve)
        .with_state(server.panel)
        // Outside every route, so it sees `/` before anything that serves it.
        .layer(middleware::from_fn_with_state(gate, auth::exchange))
        // Outside the exchange, so nothing is answered to a request that
        // names another host, not even the launch link.
        .layer(middleware::from_fn_with_state(
            Arc::new(Hosts::new(port)),
            guard::same_origin,
        ))
        // Outside the guard, so its refusals carry them too.
        .layer(middleware::from_fn(guard::restrict));
    // Outermost, so a request the guard, the gate or the exchange turns away
    // is logged too.
    Ok(match log {
        Some(log) => router.layer(middleware::from_fn_with_state(log, log::record)),
        None => router,
    })
}

/// An endpoint that answers a method it does not take in JSON, like every
/// other refusal under `/api`.
fn endpoint<S: Clone + Send + Sync + 'static>(route: MethodRouter<S>) -> MethodRouter<S> {
    route.fallback(api::not_allowed)
}
