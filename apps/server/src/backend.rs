//! What the server carries requests to.

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// The front end on the other side of the server: whatever knows what a
/// query, a task and an outcome are.
///
/// The three are associated types rather than types this crate defines, so
/// the server deserializes, hands over and serializes without ever naming
/// one. That is what lets the CLI implement this trait with its own `Task`
/// while depending on the server, and not the other way round: a protocol
/// crate holding the shared types would do the same at the price of a third
/// crate for every change to a task to pass through.
///
/// Every method is called off the async runtime: queries and previews on
/// tokio's blocking pool, runs on the one thread that does all the writing.
/// So each may block, read the disk or take its time.
///
/// Each may also refuse what it was asked ([`Refusal`]), which is how a rule
/// of the front end's, which paths a task may name or which commands run
/// without a terminal, reaches the browser without the server knowing it.
pub trait Backend: Send + Sync + 'static {
    /// A read the panel asks for.
    type Query: DeserializeOwned + Send + 'static;
    /// A change the panel asks for.
    type Task: DeserializeOwned + Send + 'static;
    /// What carrying out a task produced, sent back as it is.
    type Outcome: Serialize + Send + 'static;

    /// Answer a read.
    ///
    /// JSON rather than another associated type: each query has a shape of
    /// its own, and one type standing for all of them would be an enum whose
    /// only job is to serialize untagged.
    fn query(&self, query: Self::Query) -> Result<serde_json::Value, Refusal>;

    /// The command lines a task is, each as somebody would type it, without
    /// running any of them; or why none of them would run. The panel shows
    /// them before it asks for the run, and the transcript records them
    /// beside what the run produced.
    ///
    /// By reference, because the server asks for the lines of the very task
    /// it is about to run, and a task need not be `Clone` to be both shown
    /// and carried out.
    fn preview(&self, task: &Self::Task) -> Result<Vec<String>, Refusal>;

    /// Carry out a task, or refuse it with nothing of it run.
    ///
    /// The server asks for the task's preview first, on the same thread,
    /// and runs nothing whose preview was refused. A backend holds the task
    /// to its rules again all the same: the disk a run meets is the one it
    /// meets, and nothing promises a caller of this trait asked first.
    fn run(&self, task: Self::Task) -> Result<Self::Outcome, Refusal>;

    /// The files and folders whose changes mean what the panel shows is out
    /// of date, each with what it is of ([`Watched`]).
    ///
    /// Which paths those are is the front end's to say, since they are its
    /// rules: what a project holds, and where its agents read. Asked when
    /// the server starts and again after every change it reports, since a
    /// change can name new ones: a project linked, a folder made.
    fn watched_paths(&self) -> Vec<Watched>;
}

/// One path a backend asks the server to watch, and what a change to it is
/// a change to.
///
/// Named, never a tree: a file is watched for its own changes and a folder
/// for what is directly in it, nothing further down, so the backend names
/// each folder it reads. One that is not there yet is watched for its being
/// made, from the nearest folder above it that is.
#[derive(Debug, Clone, PartialEq)]
pub struct Watched {
    pub path: PathBuf,
    /// The backend's own word for what the path is of, carried to the panel
    /// in the event that says it changed and never read here: JSON, as a
    /// query's answer is, so the server names no type of the front end's.
    pub scope: serde_json::Value,
}

/// Why a backend will not do what it was asked, though it read the request:
/// a value the front end's rules do not let a browser name, or a task no
/// front end without a terminal of its own runs.
///
/// A code beside the message, as every error core gives has one: the panel
/// acts on a condition by its code, where the message is reworded whenever
/// it is made clearer. A struct of the server's own rather than a fourth
/// associated type, because the server builds the reply from it, a 422 of
/// `{"error": <message>, "code": <code>}`, the shape of every refusal under
/// `/api` with the code added, and knows nothing more of it than that. A
/// code is a fixed word, never one built at run time, hence `'static`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
}
