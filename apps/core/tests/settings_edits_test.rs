//! A change of a project's settings, held as properties over changes and
//! files drawn afresh: whatever is changed, what somebody wrote in the file
//! that the change is not about is still there, word for word.
//!
//! `agents`, `share` and `targets` go through one rewrite (`change_settings`,
//! `settings::edited`), which every command that changes a project's settings
//! reuses, so a change to it made for one key that loses a comment or a key
//! this build does not know fails here for all of them.

use std::fs;

use mindflayer_core::mother::random;
use mindflayer_core::mother::{ProjectIdMother, ProjectMother};
use mindflayer_core::{Agent, MindProject, Settings, Share, WorkspaceError};

const ROUNDS: usize = 40;

/// A project marked by its settings in a folder of its own.
fn project() -> (tempfile::TempDir, MindProject) {
    let dir = tempfile::TempDir::new().unwrap();
    let project = ProjectMother::random(dir.path());
    (dir, project)
}

/// What somebody adds to a settings file by hand: a comment of their own
/// and a top-level key a newer build may know, which goes at the top, ahead
/// of the first table, where a key is no table's.
fn written_by_hand() -> (String, String) {
    let comment = format!("# {}", random::text(40).replace(['\n', '\r'], " "));
    let key = format!("x_{} = \"{}\"", random::word(), random::word());
    (comment, key)
}

fn settings_text(project: &MindProject) -> String {
    fs::read_to_string(project.marker_path()).unwrap()
}

fn read(project: &MindProject) -> Settings {
    Settings::read(&project.marker_path()).unwrap()
}

/// Agents other than `these`, at least one, in an order of their own.
fn other_agents(these: &[Agent]) -> Vec<Agent> {
    loop {
        let mut drawn = random::some_of(&Agent::ALL);
        if random::chance() {
            drawn.reverse();
        }
        if !drawn.is_empty() && drawn != these {
            return drawn;
        }
    }
}

#[test]
fn a_change_of_agents_keeps_every_comment_and_unknown_key_somebody_wrote() {
    for _ in 0..ROUNDS {
        let (_dir, mut project) = project();
        let (comment, key) = written_by_hand();
        let path = project.marker_path();
        let with_hand = format!("{comment}\n{key}\n{}", settings_text(&project));
        fs::write(&path, &with_hand).unwrap();
        let before = read(&project);
        let wanted = other_agents(&before.agents);

        project
            .change_settings(|settings| settings.agents = wanted.clone())
            .unwrap();

        let text = settings_text(&project);
        assert!(text.contains(&comment), "{comment:?} lost from:\n{text}");
        assert!(text.contains(&key), "{key:?} lost from:\n{text}");
        let after = read(&project);
        assert_eq!(after.agents, wanted, "{text}");
        assert_eq!(
            Settings {
                agents: before.agents.clone(),
                ..after
            },
            before,
            "more than the agents changed:\n{text}"
        );
    }
}

#[test]
fn every_comment_the_template_wrote_survives_any_change() {
    for _ in 0..ROUNDS {
        let (_dir, mut project) = project();
        let original = settings_text(&project);
        let comments: Vec<&str> = original
            .lines()
            .filter(|line| line.trim_start().starts_with('#'))
            .collect();
        let before = read(&project);

        project
            .change_settings(|settings| {
                settings.agents = other_agents(&before.agents);
                settings.share = random::pick(&[Share::Local, Share::Shared]);
                settings.edit_tracked = random::chance();
            })
            .unwrap();

        let text = settings_text(&project);
        for comment in comments {
            assert!(text.contains(comment), "{comment:?} lost from:\n{text}");
        }
    }
}

#[test]
fn a_change_that_changes_nothing_writes_nothing() {
    for _ in 0..ROUNDS {
        let (_dir, mut project) = project();
        let (comment, key) = written_by_hand();
        let with_hand = format!("{comment}\n{key}\n{}", settings_text(&project));
        fs::write(project.marker_path(), &with_hand).unwrap();

        project.change_settings(|_| ()).unwrap();
        let same = read(&project);
        project
            .change_settings(|settings| *settings = same.clone())
            .unwrap();

        assert_eq!(settings_text(&project), with_hand);
    }
}

#[test]
fn a_change_of_the_id_or_the_version_is_refused_and_the_file_is_left_alone() {
    for _ in 0..ROUNDS {
        let (_dir, mut project) = project();
        let original = settings_text(&project);

        let id = project.change_settings(|settings| settings.id = ProjectIdMother::random());
        let version = project.change_settings(|settings| settings.version += 1);

        assert!(matches!(id, Err(WorkspaceError::Edit { .. })), "{id:?}");
        assert!(
            matches!(version, Err(WorkspaceError::Edit { .. })),
            "{version:?}"
        );
        assert_eq!(settings_text(&project), original);
    }
}

#[test]
fn changes_made_one_after_another_end_where_the_last_one_says() {
    for _ in 0..ROUNDS {
        let (_dir, mut project) = project();
        let mut expected = read(&project);

        for _ in 0..random::between(2, 5) {
            let agents = other_agents(&expected.agents);
            let share = random::pick(&[Share::Local, Share::Shared]);
            let edit_tracked = random::chance();
            project
                .change_settings(|settings| {
                    settings.agents = agents.clone();
                    settings.share = share;
                    settings.edit_tracked = edit_tracked;
                })
                .unwrap();
            expected.agents = agents;
            expected.share = share;
            expected.edit_tracked = edit_tracked;
        }

        assert_eq!(read(&project), expected, "{}", settings_text(&project));
        assert_eq!(project.settings(), Some(&expected));
    }
}
