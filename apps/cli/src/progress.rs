//! What a gather is doing, in words, and drawn on the terminal while it runs.
//!
//! Core says which step a gather is at ([`Progress`]); how that reads is the
//! front end's to decide, and both of this crate's say it with [`said`]: the
//! line `flayer gather` keeps rewriting on stderr, and the box the TUI draws
//! while a gather runs.

use std::io::{self, IsTerminal as _, Write};
use std::sync::{Arc, Mutex};

use mindflayer_core::gather::{Progress, Watch};

/// One step of a gather, as a person reads it: what is being counted, and
/// how far it has got.
pub(crate) fn said(progress: Progress) -> String {
    let (what, done, total) = match progress {
        Progress::Connecting => return String::from("connecting"),
        Progress::Receiving { done, total } => ("receiving objects", done, total),
        Progress::Resolving { done, total } => ("resolving deltas", done, total),
        Progress::CheckingOut { done, total } => ("checking out files", done, total),
        Progress::Harvesting { done, total } => ("reading folders", done, Some(total)),
        Progress::Placing { done, total } => ("placing on the shelf", done, Some(total)),
    };
    match total {
        Some(total) => format!("{what} {done}/{total}"),
        // A count with nothing to measure it against says little until it
        // has started.
        None if done == 0 => what.to_owned(),
        None => format!("{what} {done}"),
    }
}

/// A gather's progress, drawn on one line of stderr and rewritten in place as
/// it changes, then wiped before the command says what it did.
///
/// Only when stderr is a terminal. Piped, or read by a test, a line rewritten
/// with carriage returns is noise in a log, and the report that follows says
/// what happened anyway; so is anything a command run that way prints, which
/// is why it is not printed at all rather than printed differently.
pub struct ProgressLine {
    drawn: Option<Arc<Mutex<Drawn>>>,
}

impl ProgressLine {
    /// A line on stderr, drawn if stderr is a terminal and never otherwise.
    pub fn on_stderr() -> Self {
        if io::stderr().is_terminal() {
            Self::to(io::stderr())
        } else {
            Self { drawn: None }
        }
    }

    /// A line drawn into `out`, whatever it is.
    fn to(out: impl Write + Send + 'static) -> Self {
        Self {
            drawn: Some(Arc::new(Mutex::new(Drawn {
                out: Box::new(out),
                width: 0,
            }))),
        }
    }

    /// The watch to run a command with, so what it gathers is drawn here.
    pub fn watch(&self) -> Watch {
        let Some(drawn) = &self.drawn else {
            return Watch::new();
        };
        let drawn = Arc::clone(drawn);
        Watch::new().reporting_to(move |progress| {
            if let Ok(mut drawn) = drawn.lock() {
                drawn.draw(&said(progress));
            }
        })
    }

    /// Wipe the line, so what the command says next starts at the beginning
    /// of an empty one rather than after the last step it reached.
    pub fn clear(&self) {
        if let Some(Ok(mut drawn)) = self.drawn.as_ref().map(|drawn| drawn.lock()) {
            drawn.wipe();
        }
    }
}

/// Where the line goes, and how much of it is drawn there now.
struct Drawn {
    out: Box<dyn Write + Send>,
    /// In characters, which is what a terminal advances by for these words.
    width: usize,
}

impl Drawn {
    /// Draw `text` over what is there. A carriage return and spaces over the
    /// rest, rather than an escape that erases the line, which a terminal
    /// that does not read escapes would print as it is.
    fn draw(&mut self, text: &str) {
        let width = text.chars().count();
        let rest = self.width.saturating_sub(width);
        // A terminal that has gone away is no reason to stop a gather.
        let _ = write!(self.out, "\r{text}{:rest$}", "");
        let _ = self.out.flush();
        self.width = width;
    }

    fn wipe(&mut self) {
        if self.width == 0 {
            return;
        }
        let _ = write!(self.out, "\r{:width$}\r", "", width = self.width);
        let _ = self.out.flush();
        self.width = 0;
    }
}

#[cfg(test)]
#[path = "progress_test.rs"]
mod tests;
