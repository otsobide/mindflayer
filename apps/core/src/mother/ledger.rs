//! Mothers of what the ledger records and hands back ([`crate::ledger`]),
//! each an aggregate of the value mothers, as each record is of the values.

use crate::agent::Agent;
use crate::kind::Kind;
use crate::ledger::{Entry, Gathered, InstallRecord, Moved, Source, SourceKind, Target};
use crate::mother::random;
use crate::mother::url::UrlMother;
use crate::mother::value::{
    ArtifactNameMother, ContentHashMother, OriginMother, RedactedMother, RevisionMother,
};
use crate::value::{ArtifactName, ContentHash, Origin, Redacted};

/// Makes [`Source`]s, as the ledger hands one back.
pub struct SourceMother;

impl SourceMother {
    /// A git source, at a branch or not, last gathered at some revision.
    pub fn random() -> Source {
        let reference = random::chance().then(random::word);
        Self::create(&UrlMother::clean(), reference.as_deref())
    }

    /// A git source at `url`, asked for at `reference`.
    pub fn create(url: &str, reference: Option<&str>) -> Source {
        Source {
            id: random::between(1, 1000) as i64,
            kind: SourceKind::Git,
            url: Redacted::new(url),
            reference: reference.map(str::to_owned),
            subdirectory: String::from(crate::gather::DEFAULT_SUBDIRECTORY),
            revision: Some(RevisionMother::random()),
            directory: random::word(),
        }
    }
}

/// Makes [`Gathered`] artifacts.
pub struct GatheredMother;

impl GatheredMother {
    /// A skill gathered from some source.
    pub fn random() -> Gathered {
        Self::create(
            Kind::Skill,
            ArtifactNameMother::random(),
            SourceMother::random(),
        )
    }

    /// `name`, of `kind`, gathered from `source`.
    pub fn create(kind: Kind, name: ArtifactName, source: Source) -> Gathered {
        Gathered {
            kind,
            path: format!(".mindflayer/{}/{}/{name}", kind.slug(), source.directory),
            name,
            summary: random::chance().then(|| random::text(30)),
            revision: source.revision.clone(),
            gathered_at: random::between(1_600_000_000, 1_900_000_000) as i64,
            source,
        }
    }
}

/// Makes [`Target`]s, the copies an installation wrote.
pub struct TargetMother;

impl TargetMother {
    /// A copy of a skill named `name` for one agent or another.
    pub fn random(name: &str) -> Target {
        let agent = random::pick(&Agent::ALL);
        Self::create(Some(agent), &format!(".{}/skills/{name}", agent.name()))
    }

    /// A copy at `path`, for `agent` or for no one agent.
    pub fn create(agent: Option<Agent>, path: &str) -> Target {
        Target {
            agent: agent.map(|agent| agent.name().to_owned()),
            path: path.to_owned(),
            content_hash: None,
        }
    }

    /// A copy at `path` that holds bytes of its own, as each of a rule's
    /// does.
    pub fn holding(agent: Option<Agent>, path: &str, hash: ContentHash) -> Target {
        Target {
            content_hash: Some(hash),
            ..Self::create(agent, path)
        }
    }
}

/// Makes [`InstallRecord`]s.
pub struct InstallRecordMother;

impl InstallRecordMother {
    /// A skill installed from somewhere, with a copy for each of some agents.
    pub fn random() -> InstallRecord {
        Self::skill(&ArtifactNameMother::random())
    }

    /// The skill `name`, installed from the workspace's own, a copy in each
    /// agent's folder.
    pub fn skill(name: &str) -> InstallRecord {
        Self::create(
            Kind::Skill,
            name,
            Some(OriginMother::random()),
            Some(ContentHashMother::random()),
            Agent::ALL
                .iter()
                .map(|agent| {
                    TargetMother::create(Some(*agent), &format!(".{}/skills/{name}", agent.name()))
                })
                .collect(),
        )
    }

    /// The rule `route`, installed as its section of the block and its file
    /// for Claude, each holding bytes of its own.
    pub fn rule(route: &str) -> InstallRecord {
        Self::create(
            Kind::Rule,
            route,
            Some(OriginMother::workspace()),
            Some(ContentHashMother::random()),
            vec![
                TargetMother::holding(None, "AGENTS.md", ContentHashMother::random()),
                TargetMother::holding(
                    Some(Agent::Claude),
                    &format!(".claude/rules/mindflayer/{route}.md"),
                    ContentHashMother::random(),
                ),
            ],
        )
    }

    /// A record a build from before schema 3 wrote: neither where it came
    /// from nor a hash of what it wrote.
    pub fn before_hashes(name: &str) -> InstallRecord {
        InstallRecord {
            origin: None,
            content_hash: None,
            ..Self::skill(name)
        }
    }

    /// Exactly what it is given, from no shelf source.
    pub fn create(
        kind: Kind,
        name: &str,
        origin: Option<Origin>,
        content_hash: Option<ContentHash>,
        targets: Vec<Target>,
    ) -> InstallRecord {
        InstallRecord {
            kind,
            name: ArtifactName::from(name),
            source_id: None,
            origin,
            content_hash,
            targets,
        }
    }
}

/// Makes [`Moved`] installations, as a project's migration hands the ledger
/// where each one's copies are now.
pub struct MovedMother;

impl MovedMother {
    /// The skill `name`, its copies now in each agent's folder.
    pub fn skill(name: &str) -> Moved {
        Moved {
            kind: Kind::Skill,
            name: ArtifactName::from(name),
            targets: InstallRecordMother::skill(name).targets,
            content_hash: Some(ContentHashMother::random()),
        }
    }
}

/// Makes [`Entry`]s of the action log, as the ledger hands one back.
pub struct EntryMother;

impl EntryMother {
    /// Something that happened to something, successfully or not.
    pub fn random() -> Entry {
        Entry {
            at: random::between(1_600_000_000, 1_900_000_000) as i64,
            action: random::pick(&["gather", "install", "uninstall", "relink", "adopt"]).to_owned(),
            target: Some(RedactedMother::url()),
            outcome: random::pick(&["ok", "failed"]).to_owned(),
            detail: random::chance().then(RedactedMother::random),
        }
    }
}
