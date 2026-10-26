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
use crate::token::Token;
use crate::transcript::Transcript;
use crate::writer::Writer;
use crate::Backend;

/// A server not yet listening: a backend, and how to serve it.
pub struct Server<B: Backend> {
    backend: B,
}

impl<B: Backend> Server<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Take a port and issue the launch's token.
    ///
    /// 127.0.0.1 and nothing else, not even as an option: the panel is for
    /// the person at this machine, and every address beyond loopback is a
    /// network where the token would travel in clear text. Port 0, so the
    /// system picks one that is free rather than the server failing on a
    /// port somebody else took.
    ///
    /// Synchronous, and std's listener rather than tokio's, so that a front
    /// end can learn the port and print the link before it has a runtime.
    pub fn bind(self) -> io::Result<Listening> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
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
    /// rather than over the socket.
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
        // so a typo in a script gets a 404 it can read.
        .route("/api", any(api::missing))
        .route("/api/{*rest}", any(api::missing))
        // `route_layer`, so the gate guards the routes above and nothing
        // added after them.
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&gate),
            auth::require,
        ))
        .with_state(api)
        // Outside every route, so it sees `/` before anything that serves it.
        .layer(middleware::from_fn_with_state(gate, auth::exchange));
    Ok(router)
}

/// An endpoint that answers a method it does not take in JSON, like every
/// other refusal under `/api`.
fn endpoint<S: Clone + Send + Sync + 'static>(route: MethodRouter<S>) -> MethodRouter<S> {
    route.fallback(api::not_allowed)
}
