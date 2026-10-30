//! Unit tests for `task.rs`, kept beside it rather than inside it.

use std::collections::BTreeSet;

use clap::parser::ValueSource;
use clap::{ArgMatches, CommandFactory, Parser};

use super::*;
use crate::tui::minds::{Minds, Row, Standing};
use crate::{Cli, FlayerCli};

/// Every task's name in JSON, as a front end sends it: a contract, pinned
/// here word for word, so renaming a variant fails a test rather than every
/// front end that sends the old name.
const TAGS: [&str; 32] = [
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
    "marks",
    "shelf",
    "workspaces",
    "gather",
    "targets",
    "settings",
    "toggled",
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
        Task::Marks {
            marks: vec![
                Mark {
                    path: owned(),
                    change: MindsChange::InitAndLink,
                },
                Mark {
                    path: String::from("plain"),
                    change: MindsChange::Link,
                },
                Mark {
                    path: owned(),
                    change: MindsChange::Unlink,
                },
            ],
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
            Task::Update {
                holder: holder.clone(),
                reference: Some(owned()),
            },
        ]);
        // A project's settings, never the workspace's: its own reach agents
        // by being installed, and a line naming no project is refused for
        // that, by the parser or the command.
        if holder == Holder::Workspace {
            continue;
        }
        tasks.extend([
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
            Task::Toggled {
                holder: holder.clone(),
                was: toggles(&[Agent::OpenCode, Agent::Claude], Share::Shared, false),
                now: toggles(&[Agent::Codex, Agent::Claude], Share::Local, true),
            },
            Task::Toggled {
                holder: holder.clone(),
                was: toggles(&[Agent::Codex], Share::Local, false),
                now: toggles(&[], Share::Local, false),
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

/// Toggles of `agents`, shared and with tracked files left alone.
fn toggles(agents: &[Agent], share: Share, edit_tracked: bool) -> Toggles {
    Toggles {
        agents: agents.to_vec(),
        share,
        edit_tracked,
    }
}

#[test]
fn toggles_keep_the_agents_a_project_names_in_its_order_and_name_one_ticked_after_them() {
    let holder = Holder::Project(String::from("alpha"));
    let was = toggles(&[Agent::OpenCode, Agent::Claude], Share::Shared, false);
    // Ticked in any order: what the settings name keeps theirs, and one
    // more comes after them, in report order.
    let now = toggles(
        &[Agent::Codex, Agent::Claude, Agent::OpenCode],
        Share::Shared,
        false,
    );

    assert_eq!(
        Task::settings(holder.clone(), &was, &now),
        Some(Task::Settings {
            holder,
            agents: Some(AgentsLine::Named(vec![
                Agent::OpenCode,
                Agent::Claude,
                Agent::Codex,
            ])),
            share: None,
            edit_tracked: false,
        })
    );
}

#[test]
fn unticking_every_agent_is_removing_every_one_the_project_names() {
    let holder = Holder::Project(String::from("alpha"));
    let was = toggles(&[Agent::Claude, Agent::Codex], Share::Local, false);
    let now = toggles(&[], Share::Local, false);

    assert_eq!(
        Task::settings(holder.clone(), &was, &now),
        Some(Task::Settings {
            holder,
            agents: Some(AgentsLine::Removed(vec![Agent::Claude, Agent::Codex])),
            share: None,
            edit_tracked: false,
        })
    );
}

#[test]
fn a_share_line_comes_when_the_mode_or_edit_tracked_changes_and_toggles_left_as_they_were_are_no_task(
) {
    let holder = Holder::Project(String::from("alpha"));
    let was = toggles(&[Agent::Claude], Share::Local, false);

    assert_eq!(Task::settings(holder.clone(), &was, &was), None);
    // The same agents in another order are the same agents.
    let mut reordered = toggles(&[Agent::Claude, Agent::Claude], Share::Local, false);
    assert_eq!(Task::settings(holder.clone(), &was, &reordered), None);
    reordered.edit_tracked = true;
    assert_eq!(
        Task::settings(holder.clone(), &was, &reordered),
        Some(Task::Settings {
            holder,
            agents: None,
            share: Some(Share::Local),
            edit_tracked: true,
        })
    );
}

/// The minds screen, holding a row for each of `rows`, a path standing
/// as given, ticked as given.
fn screen(rows: &[(&str, Standing, bool)]) -> Minds {
    Minds {
        rows: rows
            .iter()
            .map(|(path, standing, ticked)| Row {
                path: (*path).to_owned(),
                standing: *standing,
                ticked: *ticked,
                why: String::new(),
            })
            .collect(),
        cursor: 0,
    }
}

/// What a front end sends for the marks the minds screen's rows hold, in
/// JSON as the panel sends them, each row turned over by what it offers.
fn marked(screen: &Minds) -> Task {
    let marks: Vec<serde_json::Value> = screen
        .rows
        .iter()
        .filter_map(|row| Some(serde_json::json!({"path": row.path, "change": row.change()?})))
        .collect();
    serde_json::from_value(serde_json::json!({"type": "marks", "marks": marks})).unwrap()
}

#[test]
fn marks_sent_as_json_make_the_lines_the_minds_screen_makes_for_the_same_selection() {
    let found = |mind| Standing::Found { mind };
    let linked = Standing::Linked { opens: true };
    for rows in [
        vec![
            ("/work/beta", found(true), true),
            ("/work/gamma", found(false), true),
            ("/work/alpha", linked, false),
            ("/work/delta", found(false), true),
            ("/work/kept", linked, true),
            ("/work/left", found(true), false),
        ],
        vec![("-odd", found(false), true), ("it's", found(true), true)],
        vec![("/work/alpha", linked, false)],
    ] {
        let screen = screen(&rows);

        let sent = marked(&screen);

        assert_eq!(
            sent.commands(),
            screen.task().unwrap().commands(),
            "{rows:?}"
        );
        assert_eq!(sent.cautions(), screen.task().unwrap().cautions());
    }
    assert_eq!(marked(&screen(&[])).commands(), Vec::<Vec<String>>::new());
}

#[test]
fn toggles_sent_as_json_make_the_lines_the_task_module_builds_from_them() {
    let holder = Holder::Project(String::from("alpha"));
    let was = toggles(&[Agent::OpenCode, Agent::Claude], Share::Shared, false);
    let now = toggles(&[Agent::Codex, Agent::Claude], Share::Local, true);

    let sent: Task = serde_json::from_value(serde_json::json!({
        "type": "toggled",
        "holder": {"type": "project", "entry": "alpha"},
        "was": {"agents": ["opencode", "claude"], "share": "shared", "edit_tracked": false},
        "now": {"agents": ["codex", "claude"], "share": "local", "edit_tracked": true},
    }))
    .unwrap();

    assert_eq!(
        sent.commands(),
        [
            owned(&["mind", "-C", "alpha", "agents", "--yes", "claude", "codex"]),
            owned(&[
                "mind",
                "-C",
                "alpha",
                "share",
                "--yes",
                "--edit-tracked",
                "local"
            ]),
        ]
    );
    assert_eq!(
        sent.commands(),
        Task::settings(holder.clone(), &was, &now)
            .unwrap()
            .commands()
    );
    let unchanged = Task::Toggled {
        holder,
        was: was.clone(),
        now: was,
    };
    assert_eq!(unchanged.commands(), Vec::<Vec<String>>::new());
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

/// Values that start with a dash, each one a flag or an option somewhere in
/// the grammar, or the `--` that ends them, or shaped like an option given
/// its value; then [`dashed`]'s, made up from the characters a flag is made
/// of and those a shell treats specially.
const FLAGS: [&str; 24] = [
    "-",
    "--",
    "-p",
    "-C",
    "-W",
    "-y",
    "-h",
    "-V",
    "--yes",
    "--help",
    "--version",
    "--all",
    "--force",
    "--everywhere",
    "--edit-tracked",
    "--template=full",
    "--home=",
    "-p=alpha",
    "--=",
    "-x y",
    "-;",
    "-\nflayer init",
    "--\n--yes",
    "-'quoted'",
];

/// `count` values that start with a dash, each up to a dozen characters
/// more, from a fixed seed so a failure happens again: what a property test
/// crate would generate, from no crate, since one loop over every variant is
/// all this asks of it.
///
/// No `@` among them, which with a `://` before it is a URL's credential,
/// taken out of a line where it is shown.
fn dashed(count: usize) -> Vec<String> {
    const ALPHABET: &[char] = &[
        '-', '-', '-', '=', 'p', 'C', 'W', 'y', 'e', 's', 'a', 'l', ' ', ';', '\n', '\'', '"', '&',
        '|', '$', '`', '.', '/', ':', '\\', '*', '#', '~', '\u{e9}',
    ];
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..count)
        .map(|_| {
            let length = (next() % 13) as usize;
            let mut value = String::from("-");
            value.extend((0..length).map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize]));
            value
        })
        .collect()
}

/// Every value starting with a dash the property below is held to.
fn hostile() -> Vec<String> {
    let mut values: Vec<String> = FLAGS.iter().map(|value| (*value).to_owned()).collect();
    values.extend(dashed(200));
    values
}

/// The two parsers, built once: building one is most of what parsing a line
/// costs, and the property below parses some tens of thousands.
struct Parsers {
    mind: clap::Command,
    flayer: clap::Command,
}

impl Parsers {
    fn new() -> Self {
        Parsers {
            mind: Cli::command(),
            flayer: FlayerCli::command(),
        }
    }

    /// What `line` asks for, as the parser of the binary it names reads it
    /// ([`read`]).
    fn read(&mut self, line: &[String]) -> Read {
        let command = match line[0].as_str() {
            "mind" => &mut self.mind,
            "flayer" => &mut self.flayer,
            other => panic!("{line:?} runs `{other}`"),
        };
        let matches = command
            .try_get_matches_from_mut(line)
            .unwrap_or_else(|error| panic!("{line:?} does not parse: {error}"));
        read(&[command], &line[0], &matches)
    }
}

/// What a line asked for ([`read`]): each command in turn by its name, and
/// under it each argument given.
type Read = Vec<(String, Vec<Given>)>;

/// One argument a line gave.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Given {
    id: String,
    /// The short flag that gives it, when it is an option that has one: by
    /// the flag rather than the id, since `flayer link` takes its
    /// directories as `projects` too, behind no flag and no `./`.
    short: Option<char>,
    values: Vec<String>,
}

/// What `matches` says a line asked for, `commands` the command it was read
/// by and each above it: each command in turn by its name, and under each,
/// every argument the line gave, by its id, with the values it took, as
/// given.
///
/// Two lines alike in this asked the same command for the same options in
/// the same places, each holding the same values: one that read a value as
/// an option would have an argument more, or a value fewer, one that read
/// it as a command would name another, and one that changed a value on its
/// way, a dash dropped from `--from=-odd`, would hold another under that
/// option's id, whatever the line's other fields hold.
fn read(commands: &[&clap::Command], name: &str, matches: &ArgMatches) -> Read {
    // The innermost first, and then those above it, where a global option
    // such as `-C` is declared.
    let short = |id: &str| {
        commands
            .iter()
            .rev()
            .find_map(|command| command.get_arguments().find(|arg| arg.get_id() == id))
            .and_then(clap::Arg::get_short)
    };
    let mut given: Vec<Given> = matches
        .ids()
        .filter(|id| matches.value_source(id.as_str()) == Some(ValueSource::CommandLine))
        .map(|id| Given {
            id: id.as_str().to_owned(),
            short: short(id.as_str()),
            values: matches
                .try_get_raw(id.as_str())
                .ok()
                .flatten()
                .map_or_else(Vec::new, |raw| {
                    raw.map(|value| value.to_string_lossy().into_owned())
                        .collect()
                }),
        })
        .collect();
    given.sort();
    let mut read = vec![(name.to_owned(), given)];
    if let Some((name, matches)) = matches.subcommand() {
        let inner = commands[commands.len() - 1]
            .find_subcommand(name)
            .unwrap_or_else(|| panic!("no command `{name}`"));
        let mut commands = commands.to_vec();
        commands.push(inner);
        read.extend(self::read(&commands, name, matches));
    }
    read
}

/// What the property below builds every task with in place of a value that
/// starts with a dash, to find where each field's value lands: plain, and a
/// word no fixed part of a line holds, as `plain`, which [`every`] puts
/// beside the value in some lists, does.
const GIVEN: &str = "given";

/// The short flags whose value names a project's directory: `-C`, and the
/// `-p` of an install, an uninstall and an adoption. A value that starts
/// with a dash reaches them behind a `./`, which names the same directory
/// and so the same project ([`directory`]).
const DIRECTORIES: [char; 2] = ['C', 'p'];

/// What a line built with `value` wherever [`GIVEN`] was is to be read as,
/// given how the line built with [`GIVEN`] was read (`read`): the same
/// commands, the same arguments under the same ids, and in each place
/// [`GIVEN`] was, `value` as given, behind its `./` under an option that
/// names a directory.
fn holding(read: &Read, value: &str) -> Read {
    read.iter()
        .map(|(name, given)| {
            let given = given
                .iter()
                .map(|given| {
                    let directory = given
                        .short
                        .is_some_and(|short| DIRECTORIES.contains(&short));
                    let values = given
                        .values
                        .iter()
                        .map(|held| {
                            let held = held.replace(GIVEN, value);
                            if directory && held.starts_with('-') {
                                format!("./{held}")
                            } else {
                                held
                            }
                        })
                        .collect();
                    Given {
                        values,
                        ..given.clone()
                    }
                })
                .collect();
            (name.clone(), given)
        })
        .collect()
}

#[test]
fn every_variant_s_lines_parse_with_cli_or_flayer_cli() {
    // Not the empty value: the parser refuses an empty path for what it is,
    // which is where one is to be refused.
    for value in VALUES.into_iter().filter(|value| !value.is_empty()) {
        for task in every(value) {
            for line in task.commands() {
                let error = match line[0].as_str() {
                    "mind" => Cli::try_parse_from(&line).err(),
                    "flayer" => FlayerCli::try_parse_from(&line).err(),
                    other => panic!("{task:?} runs `{other}`"),
                };
                assert!(error.is_none(), "{line:?} does not parse: {error:?}");
            }
        }
    }
}

#[test]
fn no_value_starting_with_a_dash_is_read_as_an_option_or_a_command() {
    let mut parsers = Parsers::new();
    // How each line built with a plain value is read, once.
    let plain: Vec<Vec<(Vec<String>, Read)>> = every(GIVEN)
        .iter()
        .map(|task| {
            task.commands()
                .into_iter()
                .map(|line| {
                    let read = parsers.read(&line);
                    (line, read)
                })
                .collect()
        })
        .collect();
    // Held to something: a field the property never reaches would pass it
    // whatever its line did with a value.
    let reached = plain
        .iter()
        .flatten()
        .flat_map(|(_, read)| read.iter().flat_map(|(_, given)| given))
        .flat_map(|given| &given.values)
        .filter(|value| value.contains(GIVEN))
        .count();
    assert!(reached > 0);
    for value in hostile() {
        for (task, expected) in every(&value).iter().zip(&plain) {
            let lines = task.commands();
            assert_eq!(lines.len(), expected.len(), "{task:?}");
            for (line, (plainly, read)) in lines.iter().zip(expected) {
                assert_eq!(
                    parsers.read(line),
                    holding(read, &value),
                    "{line:?} is not read as {plainly:?} is, holding {value:?}"
                );
            }
        }
    }
}

#[test]
fn a_line_shown_runs_as_one_command_of_the_words_it_is() {
    for value in hostile().iter().map(String::as_str).chain(VALUES) {
        for task in every(value) {
            for line in task.commands() {
                let shown = shell(&line);

                assert_eq!(words_of(&shown), line, "{shown}");
            }
        }
    }
}

/// `line`, a line [`shell`] showed, split into its words as a POSIX shell
/// would split it, refusing anything that shell would read as more than
/// words: a character outside a quote other than those `quote` leaves bare,
/// the spaces between words, and the `\'` it escapes a quote with.
fn words_of(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quoted = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                quoted = !quoted;
                word.get_or_insert_with(String::new);
            }
            c if quoted => word.get_or_insert_with(String::new).push(c),
            ' ' => words.extend(word.take()),
            '\\' => {
                let escaped = chars.next();
                assert_eq!(escaped, Some('\''), "{line}: a bare backslash");
                word.get_or_insert_with(String::new).push('\'');
            }
            c if c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c) => {
                word.get_or_insert_with(String::new).push(c);
            }
            c => panic!("{line}: {c:?} outside a quote"),
        }
    }
    assert!(!quoted, "{line}: a quote left open");
    words.extend(word);
    words
}

/// `line`, split at its spaces, as the words a front end runs.
fn line(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

#[test]
fn a_front_end_with_no_terminal_refuses_what_draws_edits_activates_gathers_or_shows() {
    for (said, refusal) in [
        ("mind", Refusal::Screen),
        ("mind tui", Refusal::Screen),
        ("mind -C alpha tui", Refusal::Screen),
        ("mind flayer", Refusal::Screen),
        ("mind flayer tui", Refusal::Screen),
        ("flayer", Refusal::Screen),
        ("flayer --home= tui", Refusal::Screen),
        ("flayer minds", Refusal::Screen),
        ("flayer minds work", Refusal::Screen),
        ("flayer install", Refusal::Screen),
        ("flayer install --from workspace", Refusal::Screen),
        ("mind flayer install", Refusal::Screen),
        ("mind edit deploy", Refusal::Editor),
        ("mind edit --editor true deploy", Refusal::Editor),
        ("flayer edit -p alpha deploy", Refusal::Editor),
        ("mind flayer edit deploy", Refusal::Editor),
        ("flayer activate", Refusal::Shell),
        ("mind flayer activate --shell fish work", Refusal::Shell),
        (
            "flayer gather git https://example.com/skills.git",
            Refusal::Unwatched,
        ),
        ("mind show deploy", Refusal::Unheld),
        ("mind -C alpha show --json rule:style", Refusal::Unheld),
        ("flayer show -p alpha deploy", Refusal::Unheld),
        ("mind flayer show deploy", Refusal::Unheld),
    ] {
        assert_eq!(refused(&line(said)), Some(refusal), "{said}");
    }
}

/// From either binary, with or without its options: a line that serves
/// the panel never ends, as the line of a front end must.
#[cfg(feature = "ui")]
#[test]
fn a_front_end_that_runs_lines_refuses_the_panel() {
    for said in [
        "flayer ui",
        "flayer ui --no-open --port 0",
        "mind ui",
        "mind -C alpha ui --dev",
        "mind flayer ui",
    ] {
        assert_eq!(refused(&line(said)), Some(Refusal::Panel), "{said}");
    }
}

/// So a build without the panel says `ui` is no command, as for any word
/// it does not know.
#[cfg(not(feature = "ui"))]
#[test]
fn without_the_ui_feature_there_is_no_ui_command() {
    for said in ["flayer ui", "mind ui", "mind flayer ui"] {
        let words = line(said);
        let parsed = match words[0].as_str() {
            "mind" => Cli::try_parse_from(&words).map(drop),
            _ => FlayerCli::try_parse_from(&words).map(drop),
        };
        let error = parsed.expect_err(said);
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::InvalidSubcommand,
            "{said}"
        );
    }
}

#[test]
fn a_front_end_with_no_terminal_runs_everything_else() {
    for said in [
        "flayer gather list",
        "flayer install -p alpha deploy",
        "flayer install --all-projects deploy",
        // Half an install, which opens no screen: the command refuses it
        // with a reason of its own.
        "flayer install -p alpha",
        "flayer install --all-projects",
        "mind install deploy",
        "mind flayer link alpha",
        "flayer which",
        "mind status --json",
        // Not a line either parser takes, which running it says, in the
        // parser's words.
        "flayer frobnicate",
        "ls",
    ] {
        assert_eq!(refused(&line(said)), None, "{said}");
    }
}

#[test]
fn a_task_is_refused_for_the_first_of_its_lines_that_is() {
    // Not the empty value, whose lines the parser refuses before this list
    // is asked: what a line that cannot run would do is no question.
    let values = VALUES.into_iter().filter(|value| !value.is_empty());
    for task in values.flat_map(every) {
        let expected = match &task {
            Task::OpenInstall => Some(Refusal::Screen),
            Task::Edit { .. } => Some(Refusal::Editor),
            Task::Gather { .. } => Some(Refusal::Unwatched),
            Task::Show { .. } => Some(Refusal::Unheld),
            _ => None,
        };

        assert_eq!(task.refusal(), expected, "{task:?}");
    }
}

/// So a front end can say which command it will not run, not only why.
#[test]
fn a_refused_task_names_the_line_it_is_refused_for() {
    let batch = Task::ApplyInstall {
        changes: vec![
            Task::Uninstall {
                projects: owned(&["alpha"]),
                kind: Kind::Skill,
                name: String::from("deploy"),
            },
            Task::OpenInstall,
        ],
    };

    assert_eq!(
        batch.refused_line(),
        Some((line("flayer install"), Refusal::Screen))
    );
    assert_eq!(Task::Shelf.refused_line(), None);
}

#[test]
fn a_refusal_says_why_by_a_code_of_its_own() {
    let codes: Vec<&str> = [
        Refusal::Screen,
        Refusal::Editor,
        Refusal::Shell,
        Refusal::Unwatched,
        Refusal::Unheld,
        Refusal::Panel,
    ]
    .iter()
    .map(Refusal::code)
    .collect();

    assert_eq!(codes, Refusal::CODES);
    assert_eq!(
        Refusal::Screen.to_string(),
        "opens a screen of its own, which needs a terminal to draw on"
    );
}

#[test]
fn a_line_that_takes_something_away_is_asked_about_for_what_it_takes() {
    use Caution::{Forces, Removes, Uninstalls, Unlinks, Unloads};
    for (said, expected) in [
        ("mind remove --yes deploy", vec![Removes]),
        ("mind -C alpha remove -y rule:style", vec![Removes]),
        ("flayer remove --yes skill:deploy", vec![Removes]),
        ("flayer remove -p alpha --yes deploy", vec![Removes]),
        ("mind flayer remove --yes deploy", vec![Removes]),
        (
            "flayer remove --everywhere --yes skill:deploy",
            vec![Removes, Uninstalls],
        ),
        ("mind uninstall deploy", vec![Uninstalls]),
        ("mind uninstall --orphaned", vec![Uninstalls]),
        ("mind uninstall --force deploy", vec![Uninstalls, Forces]),
        (
            "flayer uninstall -p alpha -p beta skill:deploy",
            vec![Uninstalls],
        ),
        (
            "flayer uninstall --all-projects --orphaned",
            vec![Uninstalls],
        ),
        (
            "flayer uninstall -p alpha --force deploy",
            vec![Uninstalls, Forces],
        ),
        ("mind install --force deploy", vec![Forces]),
        ("flayer install -p alpha --force deploy", vec![Forces]),
        ("mind update --yes --force", vec![Forces]),
        ("flayer update --all --yes --force deploy", vec![Forces]),
        ("mind unlink", vec![Unlinks]),
        ("flayer unlink alpha beta", vec![Unlinks]),
        ("mind flayer unlink alpha", vec![Unlinks]),
        ("mind unload ../team", vec![Unloads]),
        ("flayer unload team", vec![Unloads]),
    ] {
        assert_eq!(cautions(&line(said)), expected, "{said}");
    }
}

/// A dry run takes nothing away, and nor does a line that only adds or
/// carries what is installed to where its agents now read: each is shown
/// before it runs, and runs once asked.
#[test]
fn a_line_that_takes_nothing_away_is_asked_about_for_nothing() {
    for said in [
        "mind remove deploy",
        "flayer remove --everywhere skill:deploy",
        "mind update --force",
        "flayer update --all --force",
        "mind update --yes",
        "flayer install -p alpha deploy",
        "flayer link alpha",
        "mind add skill deploy Ship",
        "mind rename deploy ship",
        "flayer relink alpha beta",
        "flayer load ../team",
        "flayer adopt -p alpha --yes legacy",
        "flayer agents -p alpha --yes claude",
        "mind share --yes local",
        "mind targets set --yes skills .skills",
        "flayer migrate --library --yes",
        "flayer validate",
        "flayer init",
        // A line no parser takes would not run, and takes nothing.
        "flayer unlink",
        "flayer frobnicate",
        "ls",
    ] {
        assert_eq!(cautions(&line(said)), Vec::new(), "{said}");
    }
}

#[test]
fn a_task_is_asked_about_for_each_of_its_lines_that_takes_something_away() {
    let values = VALUES.into_iter().filter(|value| !value.is_empty());
    for task in values.flat_map(every) {
        let expected = match &task {
            Task::Remove {
                everywhere: false, ..
            } => vec![(0, Caution::Removes)],
            Task::Remove {
                everywhere: true, ..
            } => vec![(0, Caution::Removes), (0, Caution::Uninstalls)],
            Task::Uninstall { .. } => vec![(0, Caution::Uninstalls)],
            Task::Unlink { .. } => vec![(0, Caution::Unlinks)],
            Task::Unload { .. } | Task::UnloadHere { .. } => vec![(0, Caution::Unloads)],
            // `mind -C <path> init`, `flayer link` and `flayer unlink`.
            Task::Minds { .. } | Task::Marks { .. } => vec![(2, Caution::Unlinks)],
            // Its installs first, then its uninstalls, as `every` builds it.
            Task::ApplyInstall { .. } => vec![(1, Caution::Uninstalls)],
            _ => Vec::new(),
        };

        assert_eq!(task.cautions(), expected, "{task:?}");
    }
}

/// Each by a word of its own in JSON, as the panel compares it, and its
/// words for whoever is asked.
#[test]
fn a_caution_says_what_it_takes_in_a_word_and_in_words() {
    let words: Vec<serde_json::Value> = [
        Caution::Removes,
        Caution::Uninstalls,
        Caution::Unlinks,
        Caution::Unloads,
        Caution::Forces,
    ]
    .iter()
    .map(|caution| serde_json::to_value(caution).unwrap())
    .collect();

    assert_eq!(
        words,
        ["removes", "uninstalls", "unlinks", "unloads", "forces"]
    );
    assert_eq!(
        Caution::Unlinks.to_string(),
        "makes the workspace forget a project, though nothing on disk is deleted"
    );
}
