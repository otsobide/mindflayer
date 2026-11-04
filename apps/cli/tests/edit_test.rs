//! `edit --contents --if-unchanged`: the hash it is given is a SHA-256 or
//! nothing at all, told apart from one that is stale, however its letters
//! are cased, at either level and for either kind.

mod common;

use common::*;
use mindweaver_core::hash;
use mindweaver_core::mother::{
    random, ArtifactNameMother, ContentHashMother, RuleMother, SkillMother,
};

/// Where an artifact is edited: a project's own, through `mind`, or the
/// workspace's library, through `flayer`.
#[derive(Debug, Clone, Copy)]
enum Level {
    Project,
    Workspace,
}

/// One artifact of `kind`, at `level`, in a folder of its own, beside the
/// draft its contents are replaced with.
struct Edited {
    dir: TempDir,
    level: Level,
    kind: Kind,
    name: String,
    /// What it is read from: a skill's `SKILL.md`, or a rule's file.
    file: PathBuf,
}

impl Edited {
    fn new(level: Level, kind: Kind) -> Self {
        let dir = TempDir::new().unwrap();
        match level {
            Level::Project => new_project(dir.path()),
            Level::Workspace => {
                weaver(dir.path(), &["init"]).unwrap();
            }
        }
        let name = match kind {
            Kind::Skill => ArtifactNameMother::random(),
            Kind::Rule => ArtifactNameMother::route(),
        }
        .into_string();
        let edited = Edited {
            dir,
            level,
            kind,
            name,
            file: PathBuf::new(),
        };
        let added = edited
            .run(&[
                "add",
                kind.slug(),
                &edited.name,
                &SkillMother::description(),
            ])
            .unwrap();
        let file = created(&added, &format!("{} {}", kind.slug(), edited.name));
        Edited { file, ..edited }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn run(&self, args: &[&str]) -> Result<Outcome, Failure> {
        match self.level {
            Level::Project => mind(self.root(), args),
            Level::Workspace => weaver(self.root(), args),
        }
    }

    fn reference(&self) -> String {
        format!("{}:{}", self.kind.slug(), self.name)
    }

    /// Contents this artifact may be given: its own name for a skill, which
    /// `edit` holds to its folder, and something new to say either way.
    fn draft(&self) -> String {
        match self.kind {
            Kind::Skill => SkillMother::create(&self.name, &SkillMother::description()),
            Kind::Rule => RuleMother::random(),
        }
    }

    /// What the file holds now.
    fn held(&self) -> String {
        fs::read_to_string(&self.file).unwrap()
    }

    /// `edit` given `draft` whole, only while the file holds what `hash`,
    /// spelled as given, is the hash of: after an `=`, so a hash that is no
    /// hash, empty or led by a space, reaches the option as it is.
    fn edit(&self, draft: &str, hash: &str) -> Result<Outcome, Failure> {
        fs::write(self.root().join("draft.md"), draft).unwrap();
        self.run(&[
            "edit",
            &self.reference(),
            "--contents",
            "draft.md",
            &format!("--if-unchanged={hash}"),
        ])
    }
}

/// Each level with each kind.
fn every() -> Vec<(Level, Kind)> {
    [Level::Project, Level::Workspace]
        .into_iter()
        .flat_map(|level| Kind::ALL.into_iter().map(move |kind| (level, kind)))
        .collect()
}

#[test]
fn a_hash_that_is_no_sha256_is_refused_as_malformed_and_nothing_is_written() {
    for (level, kind) in every() {
        let edited = Edited::new(level, kind);
        let held = edited.held();
        for (why, text) in ContentHashMother::not_hashes() {
            let draft = edited.draft();

            let failure = edited.edit(&draft, &text).unwrap_err();

            let what = format!("{level:?} {}, {why}: {text:?}", edited.reference());
            assert_eq!(failure.error.code(), "edit_hash_malformed", "{what}");
            assert_eq!(
                failure.error.to_string(),
                format!(
                    "`{text}` is not a SHA-256, which is 64 hex digits and nothing else, so \
                     nothing was written"
                ),
                "{what}"
            );
            assert_eq!(edited.held(), held, "{what}");
        }
    }
}

#[test]
fn the_hash_of_what_the_file_holds_is_taken_however_its_letters_are_cased() {
    for (level, kind) in every() {
        let edited = Edited::new(level, kind);
        for _ in 0..random::between(2, 4) {
            let read = ContentHashMother::of(edited.held().as_bytes());
            let respelled = ContentHashMother::respelled(&read);
            let draft = edited.draft();

            let outcome = edited.edit(&draft, &respelled).unwrap_or_else(|failure| {
                panic!(
                    "{level:?} {} given {respelled}: {}",
                    edited.reference(),
                    failure.error
                )
            });

            assert_eq!(
                outcome.stdout,
                format!("edited {} {}: ok\n", kind.slug(), edited.name),
                "{level:?} given {respelled}"
            );
            assert_eq!(edited.held(), draft, "{level:?} given {respelled}");
        }
    }
}

#[test]
fn a_hash_of_what_the_file_no_longer_holds_is_stale_however_its_letters_are_cased() {
    for (level, kind) in every() {
        let edited = Edited::new(level, kind);
        let held = edited.held();
        // A hash of anything but what is there: what it held before
        // somebody else's change, as the panel read it.
        let before = ContentHashMother::random();
        assert_ne!(before, hash::bytes(held.as_bytes()));
        let respelled = ContentHashMother::respelled(&before);

        let failure = edited.edit(&edited.draft(), &respelled).unwrap_err();

        let what = format!("{level:?} {} given {respelled}", edited.reference());
        assert_eq!(failure.error.code(), "edit_stale", "{what}");
        assert!(
            failure
                .error
                .to_string()
                .ends_with("so it is not written over; read it again"),
            "{what}: {}",
            failure.error
        );
        assert_eq!(edited.held(), held, "{what}");
    }
}
