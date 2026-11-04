//! Mothers of a project's settings ([`crate::settings::Settings`]).

use crate::agent::Agent;
use crate::kind::Kind;
use crate::mother::random;
use crate::settings::{Settings, Share};
use crate::workspace::Directories;

/// Makes [`Settings`].
pub struct SettingsMother;

impl SettingsMother {
    /// A new project's settings: some agents, either share mode, and a
    /// folder for each kind.
    pub fn random() -> Settings {
        let agents = random::some_of(&Agent::ALL);
        let share = random::pick(&[Share::Local, Share::Shared]);
        Self::create(&random::kebab(), agents, share)
    }

    /// `name`'s settings, for `agents`, authoring in `skills/` and `rules/`.
    pub fn create(name: &str, agents: Vec<Agent>, share: Share) -> Settings {
        Settings::new(name, agents, share, Self::authoring())
    }

    /// The folders `mind init` gives a project that is told none (D-06).
    pub fn authoring() -> Directories {
        Directories::default()
            .with(Kind::Skill, "skills")
            .with(Kind::Rule, "rules")
    }
}
