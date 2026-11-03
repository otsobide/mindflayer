//! The reads the panel's backend serves under `/api/v1` (D-24), as the
//! table the server walks ([`Route`]): each one's method, path, parameters
//! and answer, and the [`Query`] what a request sends becomes.
//!
//! The panel's own reads, each the query the panel sends for it, and the
//! snapshot's projects, its library and one project of it besides, cut from
//! it ([`Query::Projects`], [`Query::Library`], [`Query::Project`]). An
//! identifier, a registry entry or a reference, goes in a parameter rather
//! than in the path, since either can hold a `/`. A read that takes a
//! structure no parameter holds, the boxes of a plan or a draft to judge,
//! is a POST whose body holds it, and changes nothing all the same.
//!
//! Every route, parameter and answer once served is a contract: dropping or
//! renaming one, or changing what it holds, moves the API to `/api/v2`,
//! and adding one does not. So a test pins each of them by name
//! (`routes_test`), as one pins a report's JSON.

use mindflayer_core::install::Cell;
use mindflayer_server::{Arguments, Method, Parameter, Route, Shape};
use serde::Deserialize;

use crate::backend::{Draft, Query};
use crate::task::Holder;

/// What `POST /api/v1/plan` is sent: the boxes of projects and offers,
/// ticked or not, whose plan it answers with ([`Query::Plan`]).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selected {
    pub selection: Vec<Cell>,
}

/// What `POST /api/v1/validate` is sent: a draft, and what it would be
/// written in ([`Query::Validate`]).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drafted {
    pub holder: Holder,
    pub draft: Draft,
}

/// Every read the backend serves, in the order a description of the API
/// lists them: what the snapshot holds and its parts first, then what an
/// artifact holds, what is on the disk, and what a change would do.
pub fn table() -> Vec<Route<Query>> {
    vec![
        get(
            "/snapshot",
            "Everything the workspace holds, as the panel draws it",
            Vec::new(),
            "Snapshot",
            |_| Ok(Query::Snapshot {}),
        ),
        get(
            "/projects",
            "Every entry of the workspace's registry and the project it opens, as the snapshot \
             lists them",
            Vec::new(),
            "Project[]",
            |_| Ok(Query::Projects {}),
        ),
        get(
            "/project",
            "One entry of the registry and the project it opens, as the snapshot lists it",
            vec![Parameter::required(
                "entry",
                Shape::Text,
                "The entry, as the registry spells it and the snapshot lists it",
            )],
            "Project",
            |sent| {
                let Entry { entry } = sent.read()?;
                Ok(Query::Project { entry })
            },
        ),
        get(
            "/library",
            "The workspace's own artifacts, as the snapshot lists them",
            Vec::new(),
            "Library",
            |_| Ok(Query::Library {}),
        ),
        get(
            "/files",
            "Every file and folder of one artifact, each by the route its contents are asked \
             for by",
            vec![REFERENCE, PROJECT],
            "Files",
            |sent| {
                let Artifact { reference, project } = sent.read()?;
                Ok(Query::Files {
                    holder: holder(project),
                    reference,
                })
            },
        ),
        get(
            "/contents",
            "One file of one artifact, as text inside JSON, or its size when it is no text \
             or too large",
            vec![
                REFERENCE,
                Parameter::required(
                    "file",
                    Shape::Text,
                    "The file, by its route from the artifact's folder, or a rule's own file \
                     name",
                ),
                PROJECT,
            ],
            "Contents",
            |sent| {
                let File {
                    reference,
                    file,
                    project,
                } = sent.read()?;
                Ok(Query::Contents {
                    holder: holder(project),
                    reference,
                    file,
                })
            },
        ),
        get(
            "/scan",
            "What could be linked under the folders named, as `flayer scan --suggest` finds \
             and ranks it",
            vec![
                Parameter::optional(
                    "root",
                    Shape::List,
                    "A folder to look under, by its absolute, normalized path; none for the \
                     workspace's root",
                ),
                Parameter::optional(
                    "depth",
                    Shape::Number,
                    "How many levels below each root to look; none for the command's own",
                ),
            ],
            "Scanned",
            |sent| {
                let Roots { root, depth } = sent.read()?;
                Ok(Query::Scan { roots: root, depth })
            },
        ),
        get(
            "/folders",
            "The folders directly inside one, to choose one to link or to scan under",
            vec![
                Parameter::optional(
                    "path",
                    Shape::Text,
                    "The folder, by its absolute, normalized path; none for the workspace's \
                     root",
                ),
                Parameter::optional(
                    "hidden",
                    Shape::Flag,
                    "Whether hidden folders are listed too; not unless asked",
                ),
            ],
            "Folders",
            |sent| {
                let Folder { path, hidden } = sent.read()?;
                Ok(Query::Folders { path, hidden })
            },
        ),
        get(
            "/updates",
            "What updating every project the workspace links would do, as `flayer update \
             --all` says it without `--yes`",
            Vec::new(),
            "Updates",
            |_| Ok(Query::Updates {}),
        ),
        get(
            "/workspaces",
            "Every workspace a switch may go to, the one in use and the launch's among them",
            Vec::new(),
            "Switchable[]",
            |_| Ok(Query::Workspaces {}),
        ),
        post(
            "/plan",
            "What boxes of projects and offers, ticked or not, amount to: the tasks that carry \
             them out, and what clashes and what is locked, with nothing run",
            "Selected",
            "Planned",
            |sent| {
                let Selected { selection } = sent.read()?;
                Ok(Query::Plan { selection })
            },
        ),
        post(
            "/validate",
            "What core says of an artifact a command is about to write, with nothing written",
            "Drafted",
            "Judged",
            |sent| {
                let Drafted { holder, draft } = sent.read()?;
                Ok(Query::Validate { holder, draft })
            },
        ),
    ]
}

/// The artifact a read of one names, by its reference.
const REFERENCE: Parameter = Parameter::required(
    "reference",
    Shape::Text,
    "The artifact, as `kind:name` or a name one kind holds",
);

/// The project holding it, which the workspace's own need not name.
const PROJECT: Parameter = Parameter::optional(
    "project",
    Shape::Text,
    "The project holding it, by its registry entry; none for the workspace's own",
);

/// A read by its parameters.
fn get(
    path: &'static str,
    about: &'static str,
    parameters: Vec<Parameter>,
    answers: &'static str,
    query: fn(Arguments) -> Result<Query, String>,
) -> Route<Query> {
    Route {
        method: Method::Get,
        path,
        about,
        parameters,
        body: None,
        answers,
        query,
    }
}

/// A read by its body.
fn post(
    path: &'static str,
    about: &'static str,
    body: &'static str,
    answers: &'static str,
    query: fn(Arguments) -> Result<Query, String>,
) -> Route<Query> {
    Route {
        method: Method::Post,
        path,
        about,
        parameters: Vec::new(),
        body: Some(body),
        answers,
        query,
    }
}

/// The holder a `project` parameter names: a project by its entry, or the
/// workspace's own without one, as a read of an artifact is asked of
/// either.
fn holder(project: Option<String>) -> Holder {
    project.map_or(Holder::Workspace, Holder::Project)
}

// What each route's parameters are read into, a field for each and none
// other, as the server has read them already: so a parameter the table
// names and the route does not read fails its test rather than pass
// unread.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    entry: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    reference: String,
    project: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    reference: String,
    file: String,
    project: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Roots {
    #[serde(default)]
    root: Vec<String>,
    depth: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Folder {
    path: Option<String>,
    #[serde(default)]
    hidden: bool,
}
