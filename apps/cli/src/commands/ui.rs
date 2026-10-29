//! `flayer ui` and `mind ui`: the control panel, served from this binary
//! over the CLI's backend until Ctrl-C (D-17).
//!
//! A command as every other is, reached from both binaries through the one
//! parser, with one difference it shares with the TUI: the link is written
//! as it runs rather than built and handed back, since it has to reach the
//! person, and the browser, while the server is up, which is the whole time
//! the command runs. What comes back once it stops is what the TUI hands
//! back when it closes: every line that changed something, with what each
//! said, so the session can be read and done again without the panel.

use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::Path;

use mindflayer_server::{Listening, Server};
use tokio::sync::oneshot;

use crate::backend::PanelBackend;
use crate::parse::UiOptions;
use crate::{CliError, Failure, Lookup, Outcome};

/// Where the panel's dev server serves it while it is worked on (`make
/// web/dev`), as a browser writes the origin: `server.host` and
/// `server.port` in apps/web/vite.config.ts, which takes no other port
/// (`strictPort`).
const DEV_SERVER: &str = "http://127.0.0.1:5173";

/// Where that dev server's proxy sends `/api` and the launch link (`SERVER`
/// in the same file): the port `--dev` listens on unless told another.
const DEV_PORT: u16 = 7878;

/// Serve the panel over the workspace `lookup` resolves from `directory`
/// until Ctrl-C: what `flayer ui` and `mind ui` do.
///
/// The workspace is resolved once, here, before anything listens, from the
/// `-C`, `--home`, `-W` and agent homes this command line was given
/// ([`PanelBackend::launch`]), and nothing the browser sends changes it. So
/// a workspace that cannot be used, or a port that cannot be taken, is said
/// as any command's failure is, and no link is printed.
pub(crate) fn ui_command(
    directory: &Path,
    options: &UiOptions,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let backend = PanelBackend::launch(directory, lookup)?;
    let root = backend.root().to_path_buf();
    // A clone keeps what the server's ran, for the transcript.
    let kept = backend.clone();
    let port = port(options);
    let mut server = Server::new(backend).port(port);
    if options.dev {
        server = server.dev_server(DEV_SERVER);
    }
    let listening = server.bind().map_err(|source| CliError::Unbound {
        port,
        dev: options.dev,
        source,
    })?;
    let url = listening.url();
    let address = listening.address();
    let unserved = |source| CliError::Unserved { source };
    // One thread: every query goes to tokio's blocking pool and every run to
    // the server's own writer thread, so the runtime only moves bytes.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(unserved)?;
    let served = runtime.block_on(async {
        let interrupts = Interrupts::listen()?;
        announce(
            &url,
            address,
            &root,
            options,
            &mut io::stdout(),
            &mut io::stderr(),
        )?;
        // Off the runtime, beside the server rather than before it: a text
        // browser runs in the foreground until it is closed, and would
        // otherwise wait for a server that never started.
        let no_open = options.no_open;
        tokio::task::spawn_blocking(move || {
            open(&url, no_open, &mut io::stderr(), webbrowser::open);
        });
        serve(listening, interrupts).await
    });
    // Without waiting for a browser still open in the foreground, which the
    // runtime would otherwise do before it let go.
    runtime.shutdown_background();
    served.map_err(unserved)?;
    Ok(Outcome::plain(kept.transcript()))
}

/// The port to listen on: the one named, or else the dev server's proxy's
/// under `--dev`, or else any the system picks.
fn port(options: &UiOptions) -> u16 {
    options
        .port
        .unwrap_or(if options.dev { DEV_PORT } else { 0 })
}

/// Say where the panel is.
///
/// The link alone on stdout, so a script reads it as the one line there; on
/// stderr, what is served, where, and how it stops: where, because through
/// the dev server the link names the dev server, and the port it must reach
/// is said nowhere else.
fn announce(
    url: &str,
    address: SocketAddr,
    root: &Path,
    options: &UiOptions,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<()> {
    writeln!(out, "{url}")?;
    out.flush()?;
    let through = if options.dev {
        format!(", through the panel's dev server at {DEV_SERVER} (`make web/dev`),")
    } else {
        String::new()
    };
    // Said or not, the panel is served: stderr gone is no reason to stop.
    let _ = writeln!(
        err,
        "note: serving the workspace at {} on {address}{through} until Ctrl-C",
        root.display()
    );
    Ok(())
}

/// Open the link with `opener`, unless told not to.
///
/// A browser that cannot be opened is said rather than failed on: the link
/// is there to open by hand, as on a machine with no display.
fn open(
    url: &str,
    no_open: bool,
    err: &mut dyn Write,
    opener: impl FnOnce(&str) -> io::Result<()>,
) {
    if no_open {
        return;
    }
    if let Err(error) = opener(url) {
        let _ = writeln!(
            err,
            "warning: no browser could be opened ({error}); open the link above in one"
        );
    }
}

/// Answer requests until the first Ctrl-C, then finish those under way; or,
/// at a second, stop at once.
///
/// Gracefully first, so a run the panel asked for is answered rather than
/// cut off half way, which would leave the disk as a command killed half way
/// does. The second is for whoever would rather have that than wait.
async fn serve(listening: Listening, mut interrupts: Interrupts) -> io::Result<()> {
    let (stop, stopped) = oneshot::channel::<()>();
    let serving = tokio::spawn(listening.serve(async {
        let _ = stopped.await;
    }));
    let abort = serving.abort_handle();
    tokio::spawn(async move {
        interrupts.next().await;
        let _ = stop.send(());
        interrupts.next().await;
        abort.abort();
    });
    match serving.await {
        Ok(served) => served,
        // The second Ctrl-C, which is a stop asked for.
        Err(error) if error.is_cancelled() => Ok(()),
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// Ctrl-C, listened for from the moment [`Interrupts::listen`] returns.
///
/// The platform's own listener rather than `tokio::signal::ctrl_c`, which
/// installs its handler only once it is first awaited, after the link is
/// out: a Ctrl-C pressed in between would end the process by the default
/// it replaces, rather than the way every other one does.
struct Interrupts {
    #[cfg(unix)]
    signal: tokio::signal::unix::Signal,
    #[cfg(windows)]
    signal: tokio::signal::windows::CtrlC,
}

impl Interrupts {
    /// Start listening. Inside the runtime, which delivers them.
    fn listen() -> io::Result<Self> {
        #[cfg(unix)]
        let signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        #[cfg(windows)]
        let signal = tokio::signal::windows::ctrl_c()?;
        Ok(Self { signal })
    }

    /// The next Ctrl-C; never, once the listener has stopped, which leaves
    /// whatever waits for one waiting rather than taking it for one.
    async fn next(&mut self) {
        if self.signal.recv().await.is_none() {
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod tests;
