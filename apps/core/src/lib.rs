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
pub mod frontmatter;
pub mod kind;
pub mod paths;
pub mod skill;
pub mod workspace;

pub use artifact::{Artifact, ArtifactError, Declared, ValidationIssue};
pub use catalog::{Catalog, DiscoveryFailure, Reference};
pub use frontmatter::{Document, FrontMatterError};
pub use kind::{Kind, Layout, UnknownKind};
pub use skill::{SkillManifest, MAX_DESCRIPTION_LEN, MAX_NAME_SEGMENT_LEN};
pub use workspace::{
    FlayerConfig, FlayerWorkspace, Initialization, MindConfig, MindProject, Registration,
    WorkspaceError, FLAYER_CONFIG, FLAYER_DIR, MIND_CONFIG, MIND_DIR,
};
