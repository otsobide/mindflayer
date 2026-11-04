//! An artifact goes by the name it was added under: every command that
//! lists, shows, renames or removes one agrees with `add` on it, whatever
//! name a mother draws, at either level. And a name no artifact may be
//! written under is refused, by its code, before a byte is written.

mod common;

use std::collections::BTreeMap;

use common::*;
use mindweaver_core::mother::{ArtifactNameMother, SkillMother};
use serde_json::Value;

/// Where an artifact is added: a project's own, through `mind`, or the
/// workspace's library, through `flayer`.
#[derive(Debug, Clone, Copy)]
enum Level {
    Project,
    Workspace,
}

const LEVELS: [Level; 2] = [Level::Project, Level::Workspace];

/// A project or a workspace in a folder of its own.
struct Place {
    dir: TempDir,
    level: Level,
}

impl Place {
    fn new(level: Level) -> Self {
        let dir = TempDir::new().unwrap();
        match level {
            Level::Project => new_project(dir.path()),
            Level::Workspace => {
                flayer(dir.path(), &["init"]).unwrap();
            }
        }
        Place { dir, level }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// What a front end's task names this level by, from its root.
    fn holder(&self) -> Holder {
        match self.level {
            Level::Project => Holder::Project(String::from(".")),
            Level::Workspace => Holder::Workspace,
        }
    }

    /// The one line `task` is, run here.
    fn ran(&self, task: Task) -> Result<Outcome, Failure> {
        let lines = task.commands();
        assert_eq!(lines.len(), 1, "{task:?}");
        typed(self.root(), &lines[0])
    }

    /// `args` run by the command of this level.
    fn run(&self, args: &[&str]) -> Result<Outcome, Failure> {
        match self.level {
            Level::Project => mind(self.root(), args),
            Level::Workspace => flayer(self.root(), args),
        }
    }

    /// Where this level keeps `kind`, asked of it.
    fn folder(&self, kind: Kind) -> PathBuf {
        match self.level {
            Level::Project => directory_for(self.root(), kind),
            Level::Workspace => own_directory_for(self.root(), kind),
        }
    }

    /// What `list --json` says this level holds of its own: each artifact's
    /// kind and name, with its summary.
    fn listed(&self) -> BTreeMap<(String, String), Value> {
        let listing = self.run(&["list", "--json"]).unwrap();
        let (data, warnings): (Value, _) = enveloped(&listing);
        assert!(warnings.is_empty(), "{warnings:?}");
        let artifacts = match self.level {
            Level::Project => &data["projects"][0]["artifacts"],
            Level::Workspace => &data["library"]["artifacts"],
        };
        artifacts
            .as_array()
            .unwrap_or_else(|| panic!("no artifacts listed: {data}"))
            .iter()
            .map(|artifact| {
                let key = (
                    artifact["kind"].as_str().unwrap().to_owned(),
                    artifact["name"].as_str().unwrap().to_owned(),
                );
                (key, artifact["summary"].clone())
            })
            .collect()
    }

    /// What `show --json` says of `reference`, the one artifact it names.
    fn shown(&self, reference: &str) -> Value {
        let shown = self.run(&["show", reference, "--json"]).unwrap();
        let (data, _): (Vec<Value>, _) = enveloped(&shown);
        assert_eq!(data.len(), 1, "{reference}: {data:?}");
        data.into_iter().next().unwrap()
    }

    /// Every file under the root with what it holds, the ledger aside: what
    /// a command that writes nothing leaves as it was.
    fn files(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut found = BTreeMap::new();
        let mut folders = vec![self.root().to_path_buf()];
        while let Some(folder) = folders.pop() {
            for entry in fs::read_dir(&folder).unwrap() {
                let entry = entry.unwrap();
                let kind = entry.file_type().unwrap();
                if kind.is_dir() {
                    folders.push(entry.path());
                } else if kind.is_file() && !entry.file_name().to_string_lossy().ends_with(".db") {
                    found.insert(entry.path(), fs::read(entry.path()).unwrap());
                }
            }
        }
        found
    }
}

/// The path a report names after `prefix`, on the one line it is.
fn named_after(outcome: &Outcome, prefix: &str) -> PathBuf {
    let path = outcome
        .stdout
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix('\n'))
        .unwrap_or_else(|| panic!("not {prefix:?}: {:?}", outcome.stdout));
    PathBuf::from(path)
}

/// `count` names, no two alike, from `draw`.
fn distinct(count: usize, draw: fn() -> String) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    while names.len() < count {
        let name = draw();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn skill_name() -> String {
    ArtifactNameMother::random().into_string()
}

fn rule_route() -> String {
    ArtifactNameMother::route().into_string()
}

#[test]
fn a_skill_added_under_any_name_is_listed_shown_renamed_and_removed_by_it() {
    for level in LEVELS {
        let place = Place::new(level);
        let skills = place.folder(Kind::Skill);
        let names = distinct(6, skill_name);
        for pair in names.chunks(2) {
            let (name, to) = (&pair[0], &pair[1]);
            let description = SkillMother::description();
            let reference = format!("skill:{name}");

            let added = place
                .run(&["add", "skill", name, &description])
                .unwrap_or_else(|failure| panic!("{level:?} add {name:?}: {}", failure.error));
            let file = created(&added, &format!("skill {name}"));
            assert_eq!(file, skills.join(name).join("SKILL.md"), "{level:?} {name}");

            let listed = place.listed();
            assert_eq!(
                listed.get(&(String::from("skill"), name.clone())),
                Some(&Value::from(description.as_str())),
                "{level:?} {name}: {listed:?}"
            );
            let shown = place.shown(&reference);
            assert_eq!(shown["kind"], "skill", "{level:?} {name}");
            assert_eq!(shown["name"], name.as_str(), "{level:?} {name}");
            assert_eq!(
                shown["description"],
                description.as_str(),
                "{level:?} {name}"
            );
            assert_eq!(
                PathBuf::from(shown["file"].as_str().unwrap()),
                file,
                "{level:?} {name}"
            );

            let held = fs::read_to_string(&file).unwrap();
            let renamed = place.run(&["rename", &reference, to]).unwrap();
            assert_eq!(
                named_after(&renamed, &format!("renamed skill {name} to {to} at ")),
                skills.join(to),
                "{level:?} {name} to {to}"
            );
            assert!(!skills.join(name).exists(), "{level:?} {name}");
            // Only the `name:` line is written again, quoted when YAML would
            // otherwise read the name as a number or a word of its own.
            let moved = fs::read_to_string(skills.join(to).join("SKILL.md")).unwrap();
            let (before, after): (Vec<&str>, Vec<&str>) =
                (held.lines().collect(), moved.lines().collect());
            assert_eq!(
                before.len(),
                after.len(),
                "{level:?} {name} to {to}: {moved}"
            );
            for (was, is) in before.iter().zip(&after) {
                if was.starts_with("name:") {
                    assert!(
                        is.starts_with("name: ") && is.contains(to.as_str()),
                        "{level:?} {to}: {is}"
                    );
                } else {
                    assert_eq!(was, is, "{level:?} {name} to {to}");
                }
            }
            let listed = place.listed();
            assert!(
                listed.contains_key(&(String::from("skill"), to.clone()))
                    && !listed.contains_key(&(String::from("skill"), name.clone())),
                "{level:?} {name} to {to}: {listed:?}"
            );
            assert_eq!(place.shown(&format!("skill:{to}"))["name"], to.as_str());

            let removed = place
                .run(&["remove", "--yes", &format!("skill:{to}")])
                .unwrap();
            assert_eq!(
                named_after(&removed, &format!("removed skill {to} from ")),
                skills.join(to),
                "{level:?} {to}"
            );
            assert!(place.listed().is_empty(), "{level:?} {to}");
            assert_eq!(names_in(&skills), Vec::<String>::new(), "{level:?} {to}");
        }
    }
}

#[test]
fn a_rule_added_under_any_route_is_listed_shown_renamed_and_removed_by_it() {
    for level in LEVELS {
        let place = Place::new(level);
        let rules = place.folder(Kind::Rule);
        let routes = distinct(6, rule_route);
        for pair in routes.chunks(2) {
            let (route, to) = (&pair[0], &pair[1]);
            let description = SkillMother::description();
            let reference = format!("rule:{route}");

            let added = place
                .run(&["add", "rule", route, &description])
                .unwrap_or_else(|failure| panic!("{level:?} add {route:?}: {}", failure.error));
            let file = created(&added, &format!("rule {route}"));
            assert_eq!(file, rules.join(format!("{route}.md")), "{level:?} {route}");
            let listed = place.listed();
            assert_eq!(
                listed.get(&(String::from("rule"), route.clone())),
                Some(&Value::from(description.as_str())),
                "{level:?} {route}: {listed:?}"
            );
            let shown = place.shown(&reference);
            assert_eq!(shown["kind"], "rule", "{level:?} {route}");
            assert_eq!(shown["name"], route.as_str(), "{level:?} {route}");
            assert_eq!(
                PathBuf::from(shown["file"].as_str().unwrap()),
                file,
                "{level:?} {route}"
            );

            let renamed = place.run(&["rename", &reference, to]).unwrap();
            assert_eq!(
                named_after(&renamed, &format!("renamed rule {route} to {to} at ")),
                rules.join(format!("{to}.md")),
                "{level:?} {route} to {to}"
            );
            assert!(!file.exists(), "{level:?} {route}");
            let listed = place.listed();
            assert!(
                listed.contains_key(&(String::from("rule"), to.clone()))
                    && !listed.contains_key(&(String::from("rule"), route.clone())),
                "{level:?} {route} to {to}: {listed:?}"
            );
            assert_eq!(place.shown(&format!("rule:{to}"))["name"], to.as_str());

            let removed = place
                .run(&["remove", "--yes", &format!("rule:{to}")])
                .unwrap();
            assert_eq!(
                named_after(&removed, &format!("removed rule {to} from ")),
                rules.join(format!("{to}.md")),
                "{level:?} {to}"
            );
            assert!(place.listed().is_empty(), "{level:?} {to}");
            // The folders a route made go with its last rule.
            assert_eq!(names_in(&rules), Vec::<String>::new(), "{level:?} {to}");
        }
    }
}

/// What a refusal of a name no artifact may be written under says, by why
/// it is refused: the command's one code covers every reason, which its
/// words tell apart.
fn says_why(failure: &Failure, code: &str, why: &str, what: &str) {
    assert_eq!(failure.error.code(), code, "{what}: {}", failure.error);
    let said = failure.error.to_string();
    let limit = format!(
        "characters, over the {} character limit",
        mindweaver_core::skill::MAX_NAME_SEGMENT_LEN
    );
    let expected = match why {
        "empty" => "the name is empty",
        "a segment over the limit" => &limit,
        _ => "is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens",
    };
    assert!(said.contains(expected), "{what}: {said}");
}

#[test]
fn no_name_an_artifact_may_not_be_written_under_is_added_and_nothing_is_written() {
    for level in LEVELS {
        let place = Place::new(level);
        let before = place.files();
        for (why, name) in ArtifactNameMother::unwritable() {
            for kind in Kind::ALL {
                // As a front end builds the line, so a name led by a dash
                // reaches `add` as the name, behind a `--`.
                let added = place.ran(Task::Add {
                    holder: place.holder(),
                    kind,
                    name: name.clone(),
                    description: String::from("Does it"),
                    template: None,
                });
                let Err(failure) = added else {
                    panic!("{level:?} added {} {name:?}, {why}", kind.slug());
                };
                says_why(
                    &failure,
                    "add_name_unfit",
                    why,
                    &format!("{level:?} {} {name:?}, {why}", kind.slug()),
                );
            }
        }
        assert_eq!(place.files(), before, "{level:?}: something was written");
        assert!(place.listed().is_empty(), "{level:?}");
    }
}

#[test]
fn no_artifact_is_renamed_to_a_name_it_may_not_be_written_under() {
    for level in LEVELS {
        let place = Place::new(level);
        let (skill, rule) = (skill_name(), rule_route());
        place.run(&["add", "skill", &skill, "Does it"]).unwrap();
        place.run(&["add", "rule", &rule, "Rules it"]).unwrap();
        let before = place.files();
        for (why, name) in ArtifactNameMother::unwritable() {
            for (kind, from) in [(Kind::Skill, &skill), (Kind::Rule, &rule)] {
                let renamed = place.ran(Task::Rename {
                    holder: place.holder(),
                    kind,
                    from: from.clone(),
                    to: name.clone(),
                });
                let Err(failure) = renamed else {
                    panic!("{level:?} renamed {from} to {name:?}, {why}");
                };
                says_why(
                    &failure,
                    "rename_name_unfit",
                    why,
                    &format!("{level:?} {from} to {name:?}, {why}"),
                );
            }
        }
        assert_eq!(place.files(), before, "{level:?}: something was written");
        let listed = place.listed();
        assert_eq!(listed.len(), 2, "{level:?}: {listed:?}");
    }
}

/// Names YAML would read as a number, a boolean or nothing at all, which a
/// mother draws only now and then: each is still the name it was added
/// under, written so it reads back as text, through a rename away and back.
#[test]
fn a_name_yaml_would_read_as_another_type_is_still_that_name() {
    const NAMES: [&str; 14] = [
        "123", "007", "1e5", "0x1f", "0o17", "true", "false", "null", "no", "on", "y", "nan",
        "inf", "1-2",
    ];
    for level in LEVELS {
        let place = Place::new(level);
        for name in NAMES {
            let reference = format!("skill:{name}");
            place.run(&["add", "skill", name, "Does it"]).unwrap();
            assert_eq!(place.shown(&reference)["name"], name, "{level:?}");

            let away = format!("{name}-away");
            place.run(&["rename", &reference, &away]).unwrap();
            place
                .run(&["rename", &format!("skill:{away}"), name])
                .unwrap();

            assert_eq!(place.shown(&reference)["name"], name, "{level:?}");
            assert_eq!(
                place.listed().keys().collect::<Vec<_>>(),
                [&(String::from("skill"), name.to_owned())],
                "{level:?}"
            );
            let validated = place.run(&["validate"]).unwrap();
            assert!(validated.ok, "{level:?} {name}: {:?}", validated.stderr);
            place.run(&["remove", "--yes", &reference]).unwrap();
        }
        assert!(place.listed().is_empty(), "{level:?}");
    }
}
