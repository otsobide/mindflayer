//! Unit tests for `adopt.rs`, kept beside it rather than inside it.
//!
//! The integration suite adopts skills from projects; these hold each check
//! a copy passes before it is adopted, one at a time: a folder and no link,
//! a skill an install would write, nothing an install leaves out, a hash
//! that refuses a link inside, a name the library does not hold, and what
//! is refused before anything is read.

use super::*;

use tempfile::TempDir;

use crate::mother::random;
use crate::mother::{ProjectMother, SkillMother, WorkspaceMother};
use crate::settings::Share;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// A skill `name` in a folder of its own name under `parent`.
fn skill_in(parent: &Path, name: &str) -> PathBuf {
    let folder = parent.join(name);
    write(
        &folder.join("SKILL.md"),
        &SkillMother::create(name, &SkillMother::description()),
    );
    folder
}

#[test]
fn what_gh_skill_writes_of_where_it_got_a_skill_starts_with_github() {
    assert_eq!(PROVENANCE_PREFIX, "github-");
}

#[test]
fn a_folder_holding_a_skill_an_install_could_write_is_held_to_one() {
    let dir = TempDir::new().unwrap();
    for _ in 0..20 {
        let name = random::kebab();
        let folder = skill_in(dir.path(), &name);

        let manifest = held_to_an_install(&folder, &name).unwrap();

        assert_eq!(manifest.name, name);
        fs::remove_dir_all(&folder).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn a_copy_that_is_a_link_is_said_to_be_one_and_where_it_leads() {
    let dir = TempDir::new().unwrap();
    let real = skill_in(dir.path(), "deploy");
    let linked = dir.path().join("linked");
    std::os::unix::fs::symlink(&real, &linked).unwrap();

    let refused = held_to_an_install(&linked, "deploy").unwrap_err();

    match &refused {
        AdoptError::Unreproducible { path, reason } => {
            assert_eq!(path, &linked);
            assert_eq!(reason, &format!("it is a link to {}", real.display()));
        }
        other => panic!("not refused as unreproducible: {other:?}"),
    }
    assert_eq!(refused.code(), "adopt_unreproducible");
}

#[test]
fn a_copy_that_is_a_file_is_no_folder() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("deploy");
    write(&file, "not a folder");

    let refused = held_to_an_install(&file, "deploy").unwrap_err();

    assert!(
        matches!(&refused, AdoptError::Unreproducible { reason, .. } if reason == "it is no folder"),
        "{refused:?}"
    );
}

#[test]
fn a_copy_that_is_not_there_cannot_be_held_to_anything() {
    let dir = TempDir::new().unwrap();

    let refused = held_to_an_install(&dir.path().join("gone"), "gone").unwrap_err();

    assert!(
        matches!(refused, AdoptError::Unreproducible { .. }),
        "{refused:?}"
    );
}

#[test]
fn a_skill_breaking_a_strict_rule_is_unfit_to_adopt() {
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("deploy");
    write(
        &folder.join("SKILL.md"),
        &SkillMother::with_long_description("deploy"),
    );

    let refused = held_to_an_install(&folder, "deploy").unwrap_err();

    assert!(
        matches!(&refused, AdoptError::Unfit { name, issues } if name == "deploy" && !issues.is_empty()),
        "{refused:?}"
    );
    assert_eq!(refused.code(), "adopt_unfit");
}

/// The folder is the name an install files it under, so a skill declaring
/// another is one no install of it would write there.
#[test]
fn a_skill_declaring_a_name_other_than_its_folder_is_unfit_to_adopt() {
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("deploy");
    write(
        &folder.join("SKILL.md"),
        &SkillMother::create("release", "Ship it"),
    );

    let refused = held_to_an_install(&folder, "deploy").unwrap_err();

    assert!(matches!(refused, AdoptError::Unfit { .. }), "{refused:?}");
}

#[test]
fn a_skill_without_a_manifest_that_reads_cannot_be_adopted() {
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("deploy");
    write(
        &folder.join("SKILL.md"),
        &SkillMother::without_front_matter("deploy"),
    );

    let refused = held_to_an_install(&folder, "deploy").unwrap_err();

    assert!(matches!(refused, AdoptError::Artifact(_)), "{refused:?}");
}

#[test]
fn a_skill_holding_what_an_install_leaves_out_is_not_adopted_and_says_why() {
    let dir = TempDir::new().unwrap();
    let folder = skill_in(dir.path(), "deploy");
    write(&folder.join(".claude-plugin").join("plugin.json"), "{}");

    let refused = held_to_an_install(&folder, "deploy").unwrap_err();

    match &refused {
        AdoptError::LeftOut { path, left, reason } => {
            assert_eq!(path, &folder);
            assert_eq!(left, ".claude-plugin");
            assert!(reason.contains("plugin"), "{reason}");
        }
        other => panic!("not refused as left out: {other:?}"),
    }
    assert_eq!(refused.code(), "adopt_left_out");
}

/// Only at the skill's top: a folder of that name deeper down is one more
/// file the skill holds, which every copy holds too.
#[test]
fn what_an_install_leaves_out_further_down_is_the_skill_s_own() {
    let dir = TempDir::new().unwrap();
    let folder = skill_in(dir.path(), "deploy");
    write(
        &folder
            .join("assets")
            .join(".claude-plugin")
            .join("plugin.json"),
        "{}",
    );

    assert!(held_to_an_install(&folder, "deploy").is_ok());
}

#[test]
fn a_copy_is_told_by_the_hash_an_install_records_of_what_it_wrote() {
    let dir = TempDir::new().unwrap();
    let folder = skill_in(dir.path(), "deploy");
    write(
        &folder.join("scripts").join("run.sh"),
        &String::from_utf8_lossy(&random::bytes(32)),
    );

    assert_eq!(hash_of(&folder).unwrap(), hash::tree(&folder).unwrap());
}

#[cfg(unix)]
#[test]
fn a_copy_holding_a_link_has_no_hash_an_install_could_have_recorded() {
    let dir = TempDir::new().unwrap();
    let folder = skill_in(dir.path(), "deploy");
    std::os::unix::fs::symlink("/etc/passwd", folder.join("passwd")).unwrap();

    let refused = hash_of(&folder).unwrap_err();

    assert!(
        matches!(&refused, AdoptError::Unreproducible { path, .. } if path == &folder),
        "{refused:?}"
    );
}

#[test]
fn a_name_the_library_does_not_hold_goes_in_its_folder_for_skills() {
    let (dir, workspace) = WorkspaceMother::empty();
    let copy = skill_in(&dir.path().join("elsewhere"), "deploy");

    let library = in_library(&workspace, "deploy", &copy).unwrap();

    assert_eq!(
        library,
        workspace
            .own()
            .directory_for(Kind::Skill)
            .unwrap()
            .join("deploy")
    );
    assert!(!library.exists(), "planned, not made");
}

#[test]
fn a_name_whose_folder_the_library_holds_is_refused_naming_both() {
    let (dir, workspace) = WorkspaceMother::empty();
    let folder = workspace.own().directory_for(Kind::Skill).unwrap();
    let held = skill_in(&folder, "deploy");
    let copy = skill_in(&dir.path().join("elsewhere"), "deploy");

    let refused = in_library(&workspace, "deploy", &copy).unwrap_err();

    assert!(
        matches!(&refused, AdoptError::InLibrary { name, library, copy: from }
            if name == "deploy" && library == &held && from == &copy),
        "{refused:?}"
    );
    assert_eq!(refused.code(), "adopt_in_library");
}

/// Anything at the library's place holds it, a link to nothing included,
/// as the folder made for it with `create_dir` would refuse it.
#[cfg(unix)]
#[test]
fn a_link_to_nothing_at_the_library_s_place_holds_it() {
    let (dir, workspace) = WorkspaceMother::empty();
    let folder = workspace.own().directory_for(Kind::Skill).unwrap();
    fs::create_dir_all(&folder).unwrap();
    std::os::unix::fs::symlink(dir.path().join("nowhere"), folder.join("deploy")).unwrap();
    let copy = skill_in(&dir.path().join("elsewhere"), "deploy");

    let refused = in_library(&workspace, "deploy", &copy).unwrap_err();

    assert!(
        matches!(refused, AdoptError::InLibrary { .. }),
        "{refused:?}"
    );
}

/// One inside the other would copy a folder into itself.
#[test]
fn a_copy_holding_the_library_s_place_is_refused_as_overlapping_it() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let folder = workspace.own().directory_for(Kind::Skill).unwrap();
    fs::create_dir_all(&folder).unwrap();

    let refused = in_library(&workspace, "deploy", &folder).unwrap_err();

    assert!(
        matches!(refused, AdoptError::Install(InstallError::Overlaps { .. })),
        "{refused:?}"
    );
}

/// A rule is refused before anything is read: the ledger, the project and
/// its folders are not looked at.
#[test]
fn a_rule_is_refused_before_anything_is_read() {
    let (dir, workspace) = WorkspaceMother::empty();
    let project = ProjectMother::create(&dir.path().join("app"), &Agent::ALL, Share::Local);

    let refused = plan(
        &workspace,
        &project,
        &Reference::parse("rule:lint"),
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(
        matches!(&refused, AdoptError::Rule { name } if name == "lint"),
        "{refused:?}"
    );
    assert_eq!(refused.code(), "adopt_not_a_skill");
    assert!(!workspace.ledger_path().exists(), "no ledger made");
}

#[test]
fn a_name_that_is_not_one_folder_name_is_refused_as_unsafe() {
    let (dir, workspace) = WorkspaceMother::empty();
    let project = ProjectMother::create(&dir.path().join("app"), &Agent::ALL, Share::Local);
    for typed in ["../deploy", "ops/deploy", "skill:..", "skill:a/b", ""] {
        let refused = plan(
            &workspace,
            &project,
            &Reference::parse(typed),
            &AgentHomes::default(),
        )
        .unwrap_err();

        assert!(
            matches!(
                refused,
                AdoptError::Install(InstallError::UnsafeName { .. })
            ),
            "{typed:?} gave {refused:?}"
        );
    }
}

#[test]
fn a_skill_nowhere_an_install_would_go_names_every_folder_it_looked_in() {
    let (dir, workspace) = WorkspaceMother::empty();
    let project = ProjectMother::create(&dir.path().join("app"), &Agent::ALL, Share::Local);
    // Where the project authors, which no install writes.
    skill_in(&project.directory_for(Kind::Skill).unwrap(), "deploy");

    let refused = plan(
        &workspace,
        &project,
        &Reference::parse("deploy"),
        &AgentHomes::default(),
    )
    .unwrap_err();

    match &refused {
        AdoptError::NotHeld {
            project: named,
            name,
            folders,
        } => {
            assert_eq!(named, project.name());
            assert_eq!(name, "deploy");
            assert_eq!(folders, ".agents/skills, .claude/skills");
        }
        other => panic!("not refused as not held: {other:?}"),
    }
}

#[test]
fn a_copy_inside_an_agent_s_own_folder_is_refused_naming_the_agent() {
    let (dir, workspace) = WorkspaceMother::empty();
    let project = ProjectMother::create(&dir.path().join("app"), &[Agent::Claude], Share::Local);
    let homes = AgentHomes {
        claude_config_dir: Some(project.root().join(".claude")),
        ..AgentHomes::default()
    };

    let refused = plan(&workspace, &project, &Reference::parse("deploy"), &homes).unwrap_err();

    assert!(
        matches!(&refused, AdoptError::UserScope { reader, .. } if reader.contains("Claude")),
        "{refused:?}"
    );
    assert_eq!(refused.code(), "adopt_user_scope");
}

#[test]
fn each_adopt_error_of_its_own_has_a_code_unique_among_them() {
    let codes = AdoptError::CODES;
    let mut unique = codes.to_vec();
    unique.sort_unstable();
    unique.dedup();

    assert_eq!(unique.len(), codes.len(), "{codes:?}");
    assert_eq!(codes.len(), 12, "{codes:?}");
    assert!(
        codes.iter().all(|code| code.starts_with("adopt_")),
        "{codes:?}"
    );
}

#[test]
fn an_adopt_error_carrying_another_of_core_s_gives_its_code() {
    let error = AdoptError::from(LedgerError::Unmade {
        path: PathBuf::from("/w/.mindflayer/mindflayer.db"),
    });

    assert_eq!(error.code(), "ledger_unmade");
}

#[test]
fn copies_that_differ_name_both() {
    let error = AdoptError::Differ {
        first: PathBuf::from("/p/.claude/skills/deploy"),
        second: PathBuf::from("/p/.agents/skills/deploy"),
    };

    assert_eq!(
        error.to_string(),
        "/p/.claude/skills/deploy and /p/.agents/skills/deploy do not hold the same, so which is \
         the skill cannot be told: make them the same, or take one out, and adopt it then"
    );
    assert_eq!(error.code(), "adopt_copies_differ");
}
