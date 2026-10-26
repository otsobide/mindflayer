//! The engine every Mindflayer front end is built on.
//!
//! Mindflayer works at two levels. A **mind project** carries a `.mind`, the
//! way a repository carries a `.git`, and holds skills and rules in the
//! directories its marker names (`skills/` and `rules/` by default). A
//! **flayer workspace** carries a `.mindflayer` that references the mind
//! projects it manages, so their artifacts can be handled together. Both live
//! in [`workspace`].
//!
//! Nothing in here knows about a terminal or a window. The CLI in `apps/cli`,
//! and whatever front end comes after it, decides how to render what these
//! types return, so the two can never disagree about what a skill is.

pub mod agent;
pub mod artifact;
pub mod catalog;
pub mod copy;
pub mod create;
pub mod exclude;
pub mod frontmatter;
pub mod gather;
pub mod hash;
pub mod install;
pub mod kind;
pub mod ledger;
pub mod lifecycle;
pub mod lock;
pub mod paths;
pub mod resolve;
pub mod rule;
pub mod scan;
pub mod skill;
pub mod template;
pub mod url;
pub mod workspace;

pub use agent::AgentHomes;
pub use artifact::{Artifact, ArtifactError, Declared, ValidationIssue};
pub use catalog::{Catalog, DiscoveryFailure, Reference, QUALIFIER};
pub use create::{create, create_from, CreateError};
pub use exclude::{ExcludeError, ExcludeFile, Excluded};
pub use frontmatter::{Document, FrontMatterError};
pub use gather::{gather, GatherError, Report, Request, Source, DEFAULT_SUBDIRECTORY};
pub use install::{install, survey, uninstall, Candidate, InstallError, Standing};
pub use kind::{Kind, Layout, UnknownKind};
pub use ledger::{Ledger, LedgerError, SourceKind, LEDGER_FILE};
pub use lifecycle::{LifecycleError, Target};
pub use lock::{LockError, WorkspaceLock};
pub use resolve::{resolve, resolve_except, Inputs, Intent, Reason, ResolveError, Resolved};
pub use rule::{Lenient, RuleManifest};
pub use scan::{scan, Found, Scan, ScanFailure};
pub use skill::{
    Compatibility, Metadata, SkillManifest, MAX_COMPATIBILITY_LEN, MAX_DESCRIPTION_LEN,
    MAX_NAME_SEGMENT_LEN,
};
pub use template::{Template, TemplateError, TEMPLATES_DIR};
pub use workspace::{
    Directories, FlayerConfig, FlayerWorkspace, Initialization, Loaded, Marker, Member, MindConfig,
    MindProject, Registration, WorkspaceError, CACHE_DIR, DIRECTORIES_VERSION, FLAYER_CONFIG,
    FLAYER_DIR, MIND_CONFIG, MIND_DIR,
};
