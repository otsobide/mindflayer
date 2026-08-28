//! The engine every Mindflayer front end is built on.
//!
//! Mindflayer works at two levels. A **mind project** carries a `.mind`, the
//! way a repository carries a `.git`, and holds skills under
//! `.mind/skills/<name>/SKILL.md`. A **flayer workspace** carries a
//! `.mindflayer` that references the mind projects it manages, so their skills
//! can be handled together. Both live in [`workspace`].
//!
//! Nothing in here knows about a terminal or a window. The CLI in `apps/cli`,
//! and whatever front end comes after it, decides how to render what these
//! types return, so the two can never disagree about what a skill is.

pub mod artifact;
pub mod catalog;
pub mod copy;
pub mod frontmatter;
pub mod gather;
pub mod install;
pub mod kind;
pub mod ledger;
pub mod paths;
pub mod skill;
pub mod workspace;

pub use artifact::{Artifact, ArtifactError, Declared, ValidationIssue};
pub use catalog::{Catalog, DiscoveryFailure, Reference, QUALIFIER};
pub use frontmatter::{Document, FrontMatterError};
pub use gather::{gather, GatherError, Report, Request, Source, DEFAULT_SUBDIRECTORY};
pub use install::{install, survey, uninstall, Candidate, InstallError, Standing};
pub use kind::{Kind, Layout, UnknownKind};
pub use ledger::{Ledger, LedgerError, SourceKind, LEDGER_FILE};
pub use skill::{SkillManifest, MAX_DESCRIPTION_LEN, MAX_NAME_SEGMENT_LEN};
pub use workspace::{
    Directories, FlayerConfig, FlayerWorkspace, Initialization, MindConfig, MindProject,
    Registration, WorkspaceError, CACHE_DIR, DIRECTORIES_VERSION, FLAYER_CONFIG, FLAYER_DIR,
    MIND_CONFIG, MIND_DIR,
};
