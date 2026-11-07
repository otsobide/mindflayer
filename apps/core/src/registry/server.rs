//! What the registry answers with, as serde reads it: every key typed that
//! something here reads, and every other passed over.
//!
//! The registry's `server.json` (the schemas dated 2025-09-29 and
//! 2025-12-11) inside the API's own wrapper, `{"server": ..., "_meta":
//! ...}`, its keys in camelCase. Read leniently, as a gather reads a
//! catalog: a key it does not know is passed over, so a later schema that
//! adds one is read as this one; a key it knows left out is read as empty
//! wherever the schema gives it a default, so one missing is what the
//! mapping says of it rather than the server refused; and only a server
//! with no name or no version, or a key it knows given another shape, is
//! not read, which the listing says of it alone ([`super::Unreadable`]).
//! Nothing here is trusted, and nothing is judged: that is the mapping's.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The key under `_meta` the registry says its own of a server under, as
/// against what its publisher said.
pub(super) const OFFICIAL: &str = "io.modelcontextprotocol.registry/official";

/// One page of `/v0.1/servers`: the servers, each read on its own, and
/// where the listing goes on from.
#[derive(Debug, Deserialize)]
pub(super) struct ServerList {
    #[serde(default)]
    pub(super) servers: Vec<serde_json::Value>,
    #[serde(default)]
    pub(super) metadata: Option<Metadata>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Metadata {
    #[serde(rename = "nextCursor", default)]
    pub(super) next_cursor: Option<String>,
}

impl crate::http::Page for ServerList {
    fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }

    fn cursor(&self) -> Option<&str> {
        self.metadata.as_ref()?.next_cursor.as_deref()
    }
}

/// One version of a server as the API answers with it, alone or in a list.
#[derive(Debug, Deserialize)]
pub(super) struct Response {
    pub(super) server: Server,
    #[serde(rename = "_meta", default)]
    pub(super) meta: Option<BTreeMap<String, serde_json::Value>>,
}

/// What the registry says of a server version of its own accord, under
/// [`OFFICIAL`].
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Official {
    #[serde(default)]
    pub(super) status: Option<String>,
    #[serde(default)]
    pub(super) published_at: Option<String>,
    #[serde(default)]
    pub(super) updated_at: Option<String>,
    #[serde(default)]
    pub(super) is_latest: Option<bool>,
}

/// A server's `server.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Server {
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) title: Option<String>,
    /// Where a schema before 2025-09-29 put a server's status, which the
    /// registry's own metadata says since: read when that says none.
    #[serde(default)]
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) packages: Vec<Package>,
    #[serde(default)]
    pub(crate) remotes: Vec<Transport>,
}

/// A package a server is started from.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Package {
    #[serde(default)]
    pub(crate) registry_type: String,
    #[serde(default)]
    pub(crate) registry_base_url: Option<String>,
    #[serde(default)]
    pub(crate) identifier: String,
    #[serde(default)]
    pub(crate) version: Option<String>,
    #[serde(default)]
    pub(crate) transport: Option<Transport>,
    #[serde(default)]
    pub(crate) runtime_arguments: Vec<Argument>,
    #[serde(default)]
    pub(crate) package_arguments: Vec<Argument>,
    #[serde(default)]
    pub(crate) environment_variables: Vec<KeyValue>,
}

/// How a package's server is spoken to, or a remote server reached: its
/// `type`, and for an address its `url`, its `headers` and the variables
/// its `url` names (2025-12-11).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Transport {
    #[serde(rename = "type", default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) url: Option<String>,
    #[serde(default)]
    pub(crate) headers: Vec<KeyValue>,
    #[serde(default)]
    pub(crate) variables: BTreeMap<String, Input>,
}

/// What somebody supplies, or what is written as it is: a value with
/// `{variables}` in it, or none, which the whole of is supplied.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Input {
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) is_required: bool,
    #[serde(default)]
    pub(crate) is_secret: bool,
    #[serde(default)]
    pub(crate) value: Option<String>,
    #[serde(default)]
    pub(crate) default: Option<String>,
    /// What a form shows as an example of the value.
    #[serde(default)]
    pub(crate) placeholder: Option<String>,
    #[serde(default)]
    pub(crate) variables: BTreeMap<String, Input>,
}

/// An environment variable or a header: a name, and an [`Input`] for its
/// value.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct KeyValue {
    pub(crate) name: String,
    #[serde(flatten)]
    pub(crate) input: Input,
}

/// An argument a runtime or a package is started with: `positional`, or
/// `named` with the flag it follows.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Argument {
    #[serde(rename = "type", default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) value_hint: Option<String>,
    #[serde(flatten)]
    pub(crate) input: Input,
}
