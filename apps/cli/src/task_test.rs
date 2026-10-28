//! Unit tests for `task.rs`, kept beside it rather than inside it.

use std::collections::BTreeSet;

use clap::parser::ValueSource;
use clap::{ArgMatches, CommandFactory, Parser};

use super::*;
use crate::{Cli, FlayerCli};

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

    /// `line` as the parser of the binary it names reads it.
    fn parse(&mut self, line: &[String]) -> ArgMatches {
        let command = match line[0].as_str() {
            "mind" => &mut self.mind,
            "flayer" => &mut self.flayer,
            other => panic!("{line:?} runs `{other}`"),
        };
        command
            .try_get_matches_from_mut(line)
            .unwrap_or_else(|error| panic!("{line:?} does not parse: {error}"))
    }
}

/// What `matches` says a line asked for, every value left out: each command
/// in turn by its name, and under each, every argument the line gave, by its
/// id, with how many values it took.
///
/// Two lines alike in this asked the same command for the same options in
/// the same places: one that read a value as an option would have one more,
/// or a value fewer, and one that read it as a command would name another.
fn shape(name: &str, matches: &ArgMatches) -> Vec<(String, Vec<(String, usize)>)> {
    let mut given: Vec<(String, usize)> = matches
        .ids()
        .filter(|id| matches.value_source(id.as_str()) == Some(ValueSource::CommandLine))
        .map(|id| {
            let values = matches
                .try_get_raw(id.as_str())
                .ok()
                .flatten()
                .map_or(0, |raw| raw.len());
            (id.as_str().to_owned(), values)
        })
        .collect();
    given.sort();
    let mut shape = vec![(name.to_owned(), given)];
    if let Some((name, matches)) = matches.subcommand() {
        shape.extend(self::shape(name, matches));
    }
    shape
}

/// Every value `matches` holds, at every level, as given.
fn values(matches: &ArgMatches) -> Vec<String> {
    let mut values: Vec<String> = matches
        .ids()
        .filter_map(|id| matches.try_get_raw(id.as_str()).ok().flatten())
        .flatten()
        .map(|raw| raw.to_string_lossy().into_owned())
        .collect();
    if let Some((_, matches)) = matches.subcommand() {
        values.extend(self::values(matches));
    }
    values
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
    // Each plain line's shape, and whether it holds the plain value, once.
    let plain: Vec<Vec<_>> = every("plain")
        .iter()
        .map(|task| {
            task.commands()
                .iter()
                .map(|line| {
                    let matches = parsers.parse(line);
                    let holds = values(&matches).iter().any(|held| held.ends_with("plain"));
                    (line.clone(), shape(&line[0], &matches), holds)
                })
                .collect()
        })
        .collect();
    for value in hostile() {
        let mut spelled = vec![value.clone(), format!("./{value}")];
        spelled.extend(Kind::ALL.map(|kind| qualified(kind, &value)));
        for (task, expected) in every(&value).iter().zip(&plain) {
            let lines = task.commands();
            assert_eq!(lines.len(), expected.len(), "{task:?}");
            for (line, (plainly, shaped, holds)) in lines.iter().zip(expected) {
                let matches = parsers.parse(line);

                assert_eq!(
                    &shape(&line[0], &matches),
                    shaped,
                    "{line:?} is not read as {plainly:?} is"
                );
                // And it arrived as the value it is, wherever the plain one
                // did: as given, as a directory `./` keeps from being read as
                // a flag, or as a name its kind qualifies.
                if *holds {
                    let held = values(&matches);
                    assert!(
                        held.iter().any(|held| spelled.contains(held)),
                        "{line:?} holds {held:?}"
                    );
                }
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
fn a_front_end_with_no_terminal_refuses_what_draws_edits_activates_or_gathers() {
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
    ] {
        assert_eq!(refused(&line(said)), Some(refusal), "{said}");
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
            _ => None,
        };

        assert_eq!(task.refusal(), expected, "{task:?}");
    }
}

#[test]
fn a_refusal_says_why_by_a_code_of_its_own() {
    let codes: Vec<&str> = [
        Refusal::Screen,
        Refusal::Editor,
        Refusal::Shell,
        Refusal::Unwatched,
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
