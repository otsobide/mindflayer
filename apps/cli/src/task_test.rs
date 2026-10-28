//! Unit tests for `task.rs`, kept beside it rather than inside it.

use std::collections::BTreeSet;

use super::*;

/// Every task's name in JSON, as a front end sends it: a contract, pinned
/// here word for word, so renaming a variant fails a test rather than every
/// front end that sends the old name.
const TAGS: [&str; 30] = [
    "init_workspace",
    "init_project",
    "link",
    "link_here",
    "init_and_link",
    "unlink",
    "relink",
    "load",
    "load_here",
    "unload",
    "unload_here",
    "add",
    "show",
    "edit",
    "rename",
    "remove",
    "validate",
    "minds",
    "shelf",
    "workspaces",
    "gather",
    "targets",
    "settings",
    "update",
    "install",
    "uninstall",
    "adopt",
    "open_install",
    "apply_install",
    "refresh",
];

/// Values for every field that takes one: plain, and shaped to break a naive
/// command line or a naive reader.
const VALUES: [&str; 8] = [
    "plain",
    "-odd",
    "--yes",
    "it's 'quoted'",
    "a b\nc",
    ";",
    "",
    "\u{e9}t\u{e9}",
];

/// Every holder a task can name, `entry` the one it names a project by.
fn holders(entry: &str) -> [Holder; 4] {
    [
        Holder::Workspace,
        Holder::Project(entry.to_owned()),
        Holder::Project(String::from(".")),
        Holder::Elsewhere(entry.to_owned()),
    ]
}

/// A task of every variant, with `value` in every field that takes free text,
/// a path or a name, and each holder in turn where a variant takes one.
///
/// What the tests that hold for every task are run over, so a variant left
/// out of it is one no test covers: [`TAGS`] is checked against the names it
/// builds, and against every name serde knows, so a new variant fails until
/// it is here.
fn every(value: &str) -> Vec<Task> {
    let owned = || value.to_owned();
    let mut tasks = vec![
        Task::InitWorkspace,
        Task::InitProject,
        Task::Link { path: owned() },
        Task::LinkHere,
        Task::InitAndLink { path: owned() },
        Task::Unlink { entry: owned() },
        Task::Relink {
            old: owned(),
            new: owned(),
        },
        Task::Load { path: owned() },
        Task::LoadHere { path: owned() },
        Task::Unload { entry: owned() },
        Task::UnloadHere { path: owned() },
        Task::Minds {
            init: vec![owned()],
            link: vec![owned(), String::from("plain")],
            unlink: vec![owned()],
        },
        Task::Shelf,
        Task::Workspaces,
        Task::Gather {
            url: owned(),
            folder: owned(),
            reference: Some(owned()),
        },
        Task::Gather {
            url: owned(),
            folder: String::from(DEFAULT_SUBDIRECTORY),
            reference: None,
        },
        Task::Install {
            projects: vec![owned(), String::from("plain")],
            kind: Kind::Skill,
            name: owned(),
            from: Some(owned()),
        },
        Task::Uninstall {
            projects: vec![owned()],
            kind: Kind::Rule,
            name: owned(),
        },
        Task::Adopt {
            project: owned(),
            kind: Kind::Skill,
            name: owned(),
        },
        Task::OpenInstall,
        Task::Refresh,
        // `--everywhere` is the workspace's alone.
        Task::Remove {
            holder: Holder::Workspace,
            kind: Kind::Skill,
            name: owned(),
            everywhere: true,
        },
        Task::Update {
            holder: Holder::Workspace,
            reference: None,
        },
    ];
    tasks.push(Task::ApplyInstall {
        changes: tasks
            .iter()
            .filter(|task| matches!(task, Task::Install { .. } | Task::Uninstall { .. }))
            .cloned()
            .collect(),
    });
    for (at, holder) in holders(value).into_iter().enumerate() {
        let kind = Kind::ALL[at % Kind::ALL.len()];
        tasks.extend([
            Task::Add {
                holder: holder.clone(),
                kind,
                name: owned(),
                description: owned(),
                template: (at % 2 == 0).then(owned),
            },
            Task::Show {
                holder: holder.clone(),
                reference: owned(),
            },
            Task::Edit {
                holder: holder.clone(),
                kind,
                name: owned(),
            },
            Task::Rename {
                holder: holder.clone(),
                kind,
                from: owned(),
                to: owned(),
            },
            Task::Remove {
                holder: holder.clone(),
                kind,
                name: owned(),
                everywhere: false,
            },
            Task::Validate {
                holder: holder.clone(),
            },
            Task::Targets {
                holder: holder.clone(),
                kind: Kind::Skill,
                folders: Some(vec![owned(), String::from("plain")]),
            },
            Task::Targets {
                holder: holder.clone(),
                kind: Kind::Skill,
                folders: None,
            },
            Task::Settings {
                holder: holder.clone(),
                agents: Some(AgentsLine::Named(vec![Agent::OpenCode, Agent::Claude])),
                share: Some(Share::Local),
                edit_tracked: true,
            },
            Task::Settings {
                holder: holder.clone(),
                agents: Some(AgentsLine::Removed(vec![Agent::Codex])),
                share: None,
                edit_tracked: false,
            },
            Task::Settings {
                holder: holder.clone(),
                agents: None,
                share: Some(Share::Shared),
                edit_tracked: false,
            },
            Task::Update {
                holder,
                reference: Some(owned()),
            },
        ]);
    }
    tasks
}

/// The name a task goes by in JSON.
fn tag(task: &Task) -> String {
    let json = serde_json::to_value(task).unwrap();
    json["type"].as_str().unwrap().to_owned()
}

/// `paths` as the owned words a task holds.
fn owned(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
fn a_directory_made_a_project_is_linked_with_the_rest_in_the_order_given() {
    let task = Task::minds([
        (String::from("beta"), MindsChange::Link),
        (String::from("gamma"), MindsChange::InitAndLink),
        (String::from("alpha"), MindsChange::Unlink),
        (String::from("delta"), MindsChange::InitAndLink),
    ]);

    assert_eq!(
        task,
        Some(Task::Minds {
            init: owned(&["gamma", "delta"]),
            link: owned(&["beta", "gamma", "delta"]),
            unlink: owned(&["alpha"]),
        })
    );
    assert_eq!(
        task.unwrap().commands(),
        [
            owned(&["mind", "-C", "gamma", "init"]),
            owned(&["mind", "-C", "delta", "init"]),
            owned(&["flayer", "link", "beta", "gamma", "delta"]),
            owned(&["flayer", "unlink", "alpha"]),
        ]
    );
}

#[test]
fn nothing_to_link_or_unlink_is_no_task() {
    assert_eq!(Task::minds([]), None);
    assert_eq!(
        Task::minds([(String::from("alpha"), MindsChange::Unlink)]),
        Some(Task::Minds {
            init: Vec::new(),
            link: Vec::new(),
            unlink: owned(&["alpha"]),
        })
    );
}

#[test]
fn every_variant_is_among_the_tasks_the_tests_build() {
    let built: BTreeSet<String> = every("plain").iter().map(tag).collect();

    let pinned: BTreeSet<String> = TAGS.iter().map(|tag| (*tag).to_owned()).collect();
    assert_eq!(built, pinned);
}

#[test]
fn every_name_a_task_goes_by_in_json_is_the_one_pinned() {
    // What serde says it expected when handed a name no task has: the name
    // of every variant, which a list kept here could miss one of.
    let error = serde_json::from_str::<Task>(r#"{"type": "?"}"#)
        .unwrap_err()
        .to_string();
    let (_, expected) = error
        .split_once("expected one of ")
        .unwrap_or_else(|| panic!("{error}"));
    let known: Vec<&str> = expected.split('`').skip(1).step_by(2).collect();

    assert_eq!(known, TAGS, "{error}");
}

#[test]
fn every_task_reads_back_from_its_json_as_it_was() {
    for value in VALUES {
        for task in every(value) {
            let json = serde_json::to_string(&task).unwrap();

            let read: Task =
                serde_json::from_str(&json).unwrap_or_else(|error| panic!("{json}: {error}"));

            assert_eq!(read, task, "{json}");
        }
    }
}

#[test]
fn a_task_is_one_flat_record_tagged_by_its_name() {
    let task = Task::Settings {
        holder: Holder::Elsewhere(String::from("../collapse")),
        agents: Some(AgentsLine::Named(vec![Agent::OpenCode])),
        share: Some(Share::Local),
        edit_tracked: false,
    };

    assert_eq!(
        serde_json::to_value(&task).unwrap(),
        serde_json::json!({
            "type": "settings",
            "holder": {"type": "elsewhere", "entry": "../collapse"},
            "agents": {"type": "named", "agents": ["opencode"]},
            "share": "local",
            "edit_tracked": false,
        })
    );
    assert_eq!(
        serde_json::to_value(Task::Add {
            holder: Holder::Workspace,
            kind: Kind::Rule,
            name: String::from("git/commits"),
            description: String::from("One step at a time"),
            template: None,
        })
        .unwrap(),
        serde_json::json!({
            "type": "add",
            "holder": {"type": "workspace"},
            "kind": "rule",
            "name": "git/commits",
            "description": "One step at a time",
            "template": null,
        })
    );
    assert_eq!(
        serde_json::to_value(MindsChange::InitAndLink).unwrap(),
        "init_and_link"
    );
}

#[test]
fn a_task_sent_with_a_field_it_does_not_have_is_refused() {
    for (json, field) in [
        (
            r#"{"type": "link", "path": "beta", "force": true}"#,
            "force",
        ),
        (
            r#"{"type": "validate", "holder": {"type": "project", "entry": "alpha", "path": "x"}}"#,
            "path",
        ),
        (
            r#"{"type": "settings", "holder": {"type": "workspace"},
                "agents": {"type": "named", "agents": ["claude"], "yes": true},
                "share": null, "edit_tracked": false}"#,
            "yes",
        ),
    ] {
        let error = serde_json::from_str::<Task>(json).unwrap_err();

        assert!(
            error.to_string().contains(&format!("\"{field}\""))
                || error.to_string().contains(&format!("`{field}`")),
            "{json}: {error}"
        );
    }
}

#[test]
fn a_task_holding_a_word_no_command_line_takes_is_refused() {
    let error = serde_json::from_str::<Task>(
        r#"{"type": "edit", "holder": {"type": "workspace"}, "kind": "agent", "name": "x"}"#,
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("unknown kind `agent`"),
        "{error}"
    );
}
