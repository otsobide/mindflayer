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
    fn query(&self, query: Self::Query) -> serde_json::Value;

    /// The command lines a task is, each as somebody would type it, without
    /// running any of them. The panel shows them before it asks for the run,
    /// and the transcript records them beside what the run produced.
    ///
    /// By reference, because the server asks for the lines of the very task
    /// it is about to run, and a task need not be `Clone` to be both shown
    /// and carried out.
    fn preview(&self, task: &Self::Task) -> Vec<String>;

    /// Carry out a task.
    fn run(&self, task: Self::Task) -> Self::Outcome;

    /// The files and folders whose changes mean what the panel shows is out
    /// of date. Nothing reads them yet; live updates (#74) will watch them.
    fn watched_paths(&self) -> Vec<PathBuf>;
}
