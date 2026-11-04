//! The API's guide, `docs/api.md`, held to what the server answers: the
//! workspace it builds is built with the commands it shows, `mindweaver ui` is
//! started as it shows, on port 0 with a temporary home and every agent
//! home empty, and every `curl` line in it is sent over loopback with the
//! client `ui_test` uses (`common::panel`). Each answer is held to the
//! API's OpenAPI document (`common::contract`), and then found in what the
//! guide shows under its line, so an answer changed by hand, or one the
//! server stops giving, fails here, as the reference's walk-through of the
//! examples fails `examples_test`.
//!
//! The lines are read rather than run: curl and jq are not on every
//! machine that runs the suites, and a missing one would have to fail under
//! `$CI` and be skipped elsewhere, as a shell is in `shells_test`, for a
//! check that needs neither. Reading them asks only what the guide writes:
//! a few of curl's options and of a POSIX shell's quotes, and jq filters
//! that are paths. A line holding anything else fails, naming it, rather
//! than be sent as something it does not say.
//!
//! What differs from one run to the next is pinned, as the reference's
//! examples and the panel's fixtures pin it: the temporary home reads as
//! [`HOME`], the port as [`PORT`], the token as [`TOKEN`], and each
//! project's id as [`IDS`] says. No answer the guide shows holds a time.
//!
//! Unix alone: the guide's lines are a POSIX shell's, its paths a Unix
//! home's, and the server is stopped as it says, by `kill`.

#![cfg(all(unix, feature = "ui"))]

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::contract;
use common::panel::*;
use common::*;
use mindweaver_cli::backend::PanelBackend;
use mindweaver_server::{Backend, Condition, Own, API};
use serde_json::Value;

/// The guide, read when this suite compiles, so that changing it rebuilds
/// the suite holding it to the server.
const GUIDE: &str = include_str!("../../../docs/api.md");

/// What the temporary home reads as: the reference's, whose examples have
/// the workspace in `/home/you/Projects`.
const HOME: &str = "/home/you";

/// The port the guide's server listens on, and its token: the reference's.
const PORT: &str = "40185";
const TOKEN: &str = "bf6934705762815d0a708aceb59cb5f9f4fd65466d0f726e0b7a8d3749053513";

/// The id each project reads as, by its folder, in place of the random one
/// `mind init` gave it: the panel's fixtures' and the reference's.
const IDS: [(&str, &str); 2] = [
    ("api", "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5"),
    ("web", "0123456789abcdef0123456789abcdef"),
];

/// Where the guide has `ui` write the token file, from the home.
const TOKEN_FILE: &str = ".local/state/mindweaver.json";

/// The description the skill of the workspace's own is given.
const REVIEW: &str = "Read a change before it lands";

/// This machine's values, to send a line of the guide as it is sent here
/// and to write an answer as the guide shows it.
struct Pins {
    home: String,
    address: String,
    token: String,
    /// Each project's id, and what it reads as.
    ids: Vec<(String, &'static str)>,
}

impl Pins {
    /// `text`, something the server said, as the guide shows it.
    fn shown(&self, text: &str) -> String {
        let mut text = text
            .replace(&self.home, HOME)
            .replace(&self.address, &format!("127.0.0.1:{PORT}"))
            .replace(&self.token, TOKEN);
        for (id, pinned) in &self.ids {
            text = text.replace(id.as_str(), pinned);
        }
        text
    }

    /// `text`, a URL or a body as the guide shows it, as it is sent here.
    fn sent(&self, text: &str) -> String {
        text.replace(HOME, &self.home)
            .replace(&format!("127.0.0.1:{PORT}"), &self.address)
    }
}

// ---------------------------------------------------------------------------
// Reading the guide
// ---------------------------------------------------------------------------

/// A fenced block of the guide: what follows its opening fence, and what it
/// holds, each line ending in a newline.
struct Block {
    info: String,
    text: String,
}

/// Every fenced block of `guide`, in order. The guide opens and closes each
/// with three backticks at the start of a line, nothing nested.
fn blocks(guide: &str) -> Vec<Block> {
    let mut found = Vec::new();
    let mut open: Option<Block> = None;
    for line in guide.lines() {
        match (open.as_mut(), line.strip_prefix("```")) {
            (None, Some(info)) => {
                open = Some(Block {
                    info: info.trim().to_owned(),
                    text: String::new(),
                })
            }
            (Some(_), Some("")) => found.extend(open.take()),
            (Some(block), _) => {
                block.text.push_str(line);
                block.text.push('\n');
            }
            (None, None) => {}
        }
    }
    assert!(open.is_none(), "a block of the guide is never closed");
    found
}

/// A command a `console` block shows, its lines joined where one ends in
/// `\`, and what the block shows it printed.
#[derive(Debug)]
struct Shown {
    command: String,
    printed: String,
}

/// Every command `block`, a `console` block's text, shows: a line starting
/// with `$ ` starts one, and every line after it that does not is what it
/// printed.
fn commands(block: &str) -> Vec<Shown> {
    let mut found: Vec<Shown> = Vec::new();
    let mut continued = false;
    for line in block.lines() {
        let joined = continued;
        let part = match (joined, line.strip_prefix("$ ")) {
            (true, _) => line.trim(),
            (false, Some(command)) => command.trim(),
            (false, None) => {
                let last = found
                    .last_mut()
                    .unwrap_or_else(|| panic!("a console block starts with a command: {block}"));
                last.printed.push_str(line);
                last.printed.push('\n');
                continue;
            }
        };
        continued = part.ends_with('\\');
        let part = part.trim_end_matches('\\').trim_end();
        match found.last_mut().filter(|_| joined) {
            Some(last) => {
                last.command.push(' ');
                last.command.push_str(part);
            }
            None => found.push(Shown {
                command: part.to_owned(),
                printed: String::new(),
            }),
        }
    }
    found
}

/// `line` split into words as a POSIX shell splits it, for what the guide
/// writes: a word in single quotes as it is, one in double quotes or bare
/// with each `$name` in it given its value from `variables`, and `|` a word
/// of its own. What a shell would read as anything else fails.
fn words(line: &str, variables: &[(&str, &str)]) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            ' ' => words.extend(word.take()),
            '|' => {
                words.extend(word.take());
                words.push(String::from("|"));
            }
            '\'' => {
                let word = word.get_or_insert_with(String::new);
                loop {
                    match characters.next() {
                        Some('\'') => break,
                        Some(character) => word.push(character),
                        None => panic!("{line}: a quote is never closed"),
                    }
                }
            }
            '"' => {
                let word = word.get_or_insert_with(String::new);
                loop {
                    match characters.next() {
                        Some('"') => break,
                        Some('$') => expand(&mut characters, word, variables, line),
                        Some(character @ ('\\' | '`')) => {
                            panic!("{line}: the test reads no {character} in double quotes")
                        }
                        Some(character) => word.push(character),
                        None => panic!("{line}: a quote is never closed"),
                    }
                }
            }
            '$' => expand(
                &mut characters,
                word.get_or_insert_with(String::new),
                variables,
                line,
            ),
            '\\' | '`' | ';' | '&' | '<' | '>' | '(' | ')' | '*' | '?' | '#' | '~' => {
                panic!("{line}: the test reads no {character} outside quotes")
            }
            character => word.get_or_insert_with(String::new).push(character),
        }
    }
    words.extend(word);
    words
}

/// The value of the variable whose name follows a `$`, onto `word`.
fn expand(
    characters: &mut std::iter::Peekable<std::str::Chars<'_>>,
    word: &mut String,
    variables: &[(&str, &str)],
    line: &str,
) {
    let mut name = String::new();
    while let Some(character) = characters.next_if(|c| c.is_ascii_alphanumeric() || *c == '_') {
        name.push(character);
    }
    let value = variables
        .iter()
        .find(|(variable, _)| *variable == name)
        .map(|(_, value)| *value)
        .unwrap_or_else(|| panic!("{line}: the test knows no variable `{name}`"));
    word.push_str(value);
}

/// What a `curl` line of the guide asks, read as curl reads it.
#[derive(Debug, PartialEq)]
struct Curl {
    method: String,
    url: String,
    /// Each `-H`, as it is given.
    headers: Vec<String>,
    /// What `-d` sends, which makes the request a POST.
    data: Option<String>,
    /// What `-w` writes after the answer, as it is given.
    write_out: Option<String>,
    /// `-N`: the answer printed as it comes, a stream of events.
    streamed: bool,
    /// The jq filter the answer is piped through, if it is.
    jq: Option<String>,
}

/// `words`, a `curl` line, read as curl reads it: `-s`, `-N`, `-H`, `-d`,
/// `-w` and `-X`, one URL, and what it is piped through, jq with a filter.
fn curl(words: &[String]) -> Curl {
    let (asked, piped) = match words.iter().position(|word| word == "|") {
        Some(at) => (&words[..at], Some(&words[at + 1..])),
        None => (words, None),
    };
    assert_eq!(asked.first().map(String::as_str), Some("curl"), "{words:?}");
    let mut method = None;
    let mut url = None;
    let mut parsed = Curl {
        method: String::new(),
        url: String::new(),
        headers: Vec::new(),
        data: None,
        write_out: None,
        streamed: false,
        jq: None,
    };
    let mut rest = asked[1..].iter();
    while let Some(word) = rest.next() {
        let mut value = || {
            rest.next()
                .cloned()
                .unwrap_or_else(|| panic!("{words:?}: {word} takes a value"))
        };
        match word.as_str() {
            "-s" => {}
            "-N" => parsed.streamed = true,
            "-H" => parsed.headers.push(value()),
            "-d" => parsed.data = Some(value()),
            "-w" => parsed.write_out = Some(value()),
            "-X" => method = Some(value()),
            option if option.starts_with('-') => {
                panic!("{words:?}: the test reads no curl option {option}")
            }
            _ => {
                assert!(url.is_none(), "{words:?}: one URL to a line");
                url = Some(word.clone());
            }
        }
    }
    if let Some(piped) = piped {
        match piped {
            [jq, filter] if jq == "jq" => parsed.jq = Some(filter.clone()),
            _ => panic!("{words:?}: an answer is piped through `jq <filter>` alone"),
        }
    }
    parsed.url = url.unwrap_or_else(|| panic!("{words:?}: no URL"));
    parsed.method =
        method.unwrap_or_else(|| String::from(if parsed.data.is_some() { "POST" } else { "GET" }));
    parsed
}

/// What `jq` prints for `filter` over `value`, one value for each it
/// prints, for the filters the guide writes: a path of `.name`,
/// `["name"]`, `[n]` and `[]` steps, `.` alone the value itself.
fn jq(filter: &str, value: &Value) -> Vec<Value> {
    let mut rest = filter
        .strip_prefix('.')
        .unwrap_or_else(|| panic!("the test reads no jq filter `{filter}`"));
    let mut values = vec![value.clone()];
    while !rest.is_empty() {
        if let Some(inside) = rest.strip_prefix('[') {
            let end = inside
                .find(']')
                .unwrap_or_else(|| panic!("`{filter}`: a `[` is never closed"));
            let step = &inside[..end];
            values = if step.is_empty() {
                values
                    .iter()
                    .flat_map(|value| match value {
                        Value::Array(items) => items.clone(),
                        Value::Object(fields) => fields.values().cloned().collect(),
                        other => panic!("`{filter}`: cannot iterate over {other}"),
                    })
                    .collect()
            } else if let Some(key) = step.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                values.iter().map(|value| value[key].clone()).collect()
            } else {
                let index: usize = step
                    .parse()
                    .unwrap_or_else(|_| panic!("the test reads no jq step `[{step}]`"));
                values.iter().map(|value| value[index].clone()).collect()
            };
            rest = &inside[end + 1..];
        } else {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            assert!(!name.is_empty(), "the test reads no jq filter `{filter}`");
            values = values.iter().map(|value| value[&name].clone()).collect();
            rest = &rest[name.len()..];
        }
        rest = rest.strip_prefix('.').unwrap_or(rest);
    }
    values
}

/// Every `curl` line of the guide's `console` blocks, with what each shows
/// it printed; and every other `console` block, each whole.
fn examples(guide: &str) -> (Vec<Shown>, Vec<String>) {
    let mut lines = Vec::new();
    let mut others = Vec::new();
    for block in blocks(guide) {
        if block.info != "console" {
            // A line curl is sent in a block nobody replays would be an
            // answer nothing holds to the server.
            assert!(
                !block
                    .text
                    .lines()
                    .any(|line| line.trim_start().starts_with("curl ")),
                "a curl line outside a console block:\n{}",
                block.text
            );
            continue;
        }
        let shown = commands(&block.text);
        if shown.iter().all(|shown| shown.command.starts_with("curl ")) {
            lines.extend(shown);
        } else {
            others.push(block.text);
        }
    }
    (lines, others)
}

// ---------------------------------------------------------------------------
// Running it
// ---------------------------------------------------------------------------

/// The workspace the guide's first block builds, built as it shows in
/// `home`, and that block as the commands print it here: the repositories
/// made with gix, as `git init` makes them, and every command run in
/// process through the real parsers, with `home` as the home.
fn workspace(home: &Path) -> String {
    let projects = home.join("Projects");
    fs::create_dir_all(&projects).unwrap();
    let mut block = format!("$ cd {}\n", projects.display());
    let mut said = |command: &str, outcome: Result<Outcome, Failure>| {
        let outcome = outcome.unwrap_or_else(|failure| panic!("{command}: {failure:?}"));
        // The guide shows stdout alone, all there is only when nothing was
        // said on stderr.
        assert!(
            outcome.ok && outcome.stderr.is_empty(),
            "{command}: {outcome:?}"
        );
        block.push_str(&format!("$ {command}\n{}", outcome.stdout));
    };
    let repository = |name: &str| {
        let path = projects.join(name);
        fs::create_dir(&path).unwrap();
        gix::init(&path).expect("initialize a repository");
        path
    };

    said(
        "mindweaver init",
        weaver_at_home(&projects, home, &["init"]),
    );
    let api = repository("api");
    said(
        "git init --quiet api && mind -C api init",
        mind_at_home(&api, home, &["init"]),
    );
    said(
        "mindweaver link api",
        weaver_at_home(&projects, home, &["link", "api"]),
    );
    said(
        &format!("mindweaver add skill review '{REVIEW}'"),
        weaver_at_home(&projects, home, &["add", "skill", "review", REVIEW]),
    );
    let web = repository("web");
    said(
        "git init --quiet web && mind -C web init",
        mind_at_home(&web, home, &["init"]),
    );
    block
}

/// Each project's id here, and what it reads as.
fn ids(projects: &Path) -> Vec<(String, &'static str)> {
    IDS.iter()
        .map(|(name, pinned)| {
            let project = MindProject::open(projects.join(name)).unwrap();
            (project.settings().unwrap().id.to_string(), *pinned)
        })
        .collect()
}

/// `shown`, a `curl` line of the guide, sent to `panel` as curl sends it,
/// and what it printed held to what the guide shows; and the method and
/// the path it asked for, without its query.
fn replay(
    panel: &Panel,
    pins: &Pins,
    variables: &[(&str, &str)],
    shown: &Shown,
) -> (String, String) {
    let asked = curl(&words(&shown.command, variables));
    let url = pins.sent(&asked.url);
    let path = url
        .strip_prefix(&format!("http://{}", panel.address))
        .unwrap_or_else(|| panic!("{}: not this server's", shown.command));
    let route = path.split('?').next().unwrap_or(path).to_owned();
    if asked.streamed {
        streamed(panel, pins, shown, &asked, path);
        return (asked.method, route);
    }

    // What curl sends besides the request line, Host and the body's length,
    // which `request` sends: a body is a form unless a header says
    // otherwise, as curl says it is.
    let mut headers = vec![
        String::from("User-Agent: curl"),
        String::from("Accept: */*"),
    ];
    headers.extend(asked.headers.iter().cloned());
    let typed = asked
        .headers
        .iter()
        .any(|header| header.to_ascii_lowercase().starts_with("content-type:"));
    if asked.data.is_some() && !typed {
        headers.push(String::from(
            "Content-Type: application/x-www-form-urlencoded",
        ));
    }
    let body = asked
        .data
        .as_deref()
        .map(|data| pins.sent(data))
        .unwrap_or_default();
    let (status, _, answer) = panel.request(&asked.method, path, &headers, &body);

    let value: Value = serde_json::from_str(&answer)
        .unwrap_or_else(|error| panic!("{}: {error}: {answer}", shown.command));
    if route.starts_with(&format!("{API}/")) {
        contract::answered(&asked.method, path, status, &value);
    } else {
        // A path of no version the document describes: what every refusal
        // holds, whatever its route.
        contract::contract(|contract| contract.holds("/components/schemas/Refused", &value))
            .unwrap_or_else(|error| panic!("{}: {error}", shown.command));
    }

    let pinned = pins.shown(&answer);
    let printed = match (&asked.jq, &asked.write_out) {
        (Some(filter), None) => {
            // Shown laid out, as jq lays it out, so held to it as JSON:
            // the same values, whatever the spaces between them.
            assert_eq!(
                status, 200,
                "{}: an example not showing its status is a success: {pinned}",
                shown.command
            );
            let shown_values: Vec<Value> = serde_json::Deserializer::from_str(&shown.printed)
                .into_iter()
                .collect::<Result<_, _>>()
                .unwrap_or_else(|error| panic!("{}: {error}: {}", shown.command, shown.printed));
            let printed = jq(filter, &serde_json::from_str(&pinned).unwrap());
            let picked: Vec<String> = printed
                .iter()
                .map(|value| serde_json::to_string_pretty(value).unwrap())
                .collect();
            assert!(
                shown_values == printed,
                "{}\nshows\n{}\nwhere the server answers\n{pinned}\nof which jq picks\n{}\n",
                shown.command,
                shown.printed,
                picked.join("\n")
            );
            return (asked.method, route);
        }
        (None, Some(format)) => {
            let written = format
                .replace("\\n", "\n")
                .replace("%{http_code}", &status.to_string());
            assert!(
                !written.contains("%{") && !written.contains('\\'),
                "{}: the test writes out only \\n and %{{http_code}}",
                shown.command
            );
            format!("{pinned}{written}")
        }
        (None, None) => {
            assert_eq!(
                status, 200,
                "{}: an example not showing its status is a success: {pinned}",
                shown.command
            );
            pinned
        }
        (Some(_), Some(_)) => panic!("{}: jq would read the status as JSON", shown.command),
    };
    // What curl prints ends where the answer does; the guide puts what
    // the prompt would follow on a line of its own.
    let printed = if printed.ends_with('\n') {
        printed
    } else {
        format!("{printed}\n")
    };
    assert!(
        shown.printed == printed,
        "{}\nshows\n{}\nwhere curl prints\n{printed}",
        shown.command,
        shown.printed
    );
    (asked.method, route)
}

/// `shown`, a line reading the event stream as it comes, held to the first
/// events the stream tells: each `data:` line it shows one, with an empty
/// line after it, as the stream frames it.
fn streamed(panel: &Panel, pins: &Pins, shown: &Shown, asked: &Curl, path: &str) {
    // What `events` sends, which is all the line may ask for.
    assert_eq!(
        (asked.method.as_str(), path),
        ("GET", "/api/v1/events"),
        "{}",
        shown.command
    );
    assert_eq!(
        asked.headers,
        [format!("Authorization: Bearer {}", panel.token)],
        "{}",
        shown.command
    );
    assert_eq!(asked.jq, None, "{}", shown.command);
    let lines: Vec<&str> = shown.printed.lines().collect();
    let framed: Vec<Value> = lines
        .chunks(2)
        .map(|frame| match frame {
            [data, ""] => serde_json::from_str(
                data.strip_prefix("data: ")
                    .unwrap_or_else(|| panic!("{}: not an event: {data}", shown.command)),
            )
            .unwrap(),
            _ => panic!(
                "{}: not an event and an empty line: {frame:?}",
                shown.command
            ),
        })
        .collect();
    let read =
        |event: &Value| -> Value { serde_json::from_str(&pins.shown(&event.to_string())).unwrap() };
    let last = framed.last().cloned().expect("an event shown");

    let told = events(panel, || {}, |event| read(event) == last);

    for event in &told {
        contract::told(path, event);
    }
    let told: Vec<Value> = told.iter().take(framed.len()).map(read).collect();
    assert_eq!(told, framed, "{}", shown.command);
}

/// Every example in the guide, against the workspace it builds: what each
/// command it shows printed, `ui` started as it shows, the token file as it
/// shows it, every `curl` line's answer, and what `ui` prints once it is
/// stopped, after which the token file is gone.
#[test]
fn every_example_in_the_api_s_guide_is_what_the_server_answers() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let built = workspace(&home);
    let projects = home.join("Projects");
    let file = home.join(TOKEN_FILE);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let mut panel = serve_with(
        "mindweaver",
        &projects,
        &home,
        &["--token-file", file.to_str().unwrap()],
    );
    let pins = Pins {
        home: home.display().to_string(),
        address: panel.address.to_string(),
        token: panel.token.clone(),
        ids: ids(&projects),
    };
    let handed = fs::read_to_string(&file).unwrap();
    let read: Value = serde_json::from_str(&handed).unwrap();
    let variables = [
        ("api", read["api"].as_str().unwrap()),
        ("token", read["token"].as_str().unwrap()),
    ];
    let (lines, others) = examples(GUIDE);

    let mut asked = BTreeSet::new();
    for shown in &lines {
        asked.insert(replay(&panel, &pins, &variables, shown));
    }
    let (exited, rest, said) = signal(&mut panel, "-TERM");

    assert!(exited.success(), "{exited:?}: {said}");
    assert!(
        fs::symlink_metadata(&file).is_err(),
        "the token file is still there"
    );
    // The guide says these in this order, each a block of its own.
    let at = format!("{HOME}/{TOKEN_FILE}");
    let expected = [
        pins.shown(&built),
        format!(
            "$ mindweaver ui --no-open --token-file {at}\n{}\n{}",
            pins.shown(&format!("http://{}/?token={}", panel.address, panel.token)),
            pins.shown(&said)
        ),
        format!(
            "$ cat {at}\n{}$ api=$(jq -r .api {at})\n$ token=$(jq -r .token {at})\n",
            pins.shown(&handed)
        ),
    ];
    assert!(
        others == expected,
        "docs/api.md should show, each in a console block of its own and nothing else \
         in one but curl lines:\n{}",
        expected.join("\n")
    );
    let stopped = pins.shown(&rest);
    assert!(
        GUIDE.contains(&format!("\n```\n{stopped}```\n")),
        "docs/api.md should show what ui prints once it stops, in a block of its own:\n{stopped}"
    );
    // Every route has an example, the server's own among them.
    let routes = PanelBackend::routes()
        .into_iter()
        .map(|route| (route.method.as_str(), route.path))
        .chain(Own::ALL.map(|own| (own.method().as_str(), own.path())));
    for (method, path) in routes {
        let route = (method.to_owned(), format!("{API}{path}"));
        assert!(asked.contains(&route), "no example of {method} {path}");
    }
}

/// The table of the server's codes in the guide holds every one, each
/// with the status it is answered with, and none it does not give.
#[test]
fn the_guide_lists_every_code_the_server_gives_with_its_status() {
    let listed: BTreeSet<(u16, &str)> = GUIDE
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let status = cells.get(1)?.parse().ok()?;
            let code = cells.get(2)?.strip_prefix('`')?.strip_suffix('`')?;
            Some((status, code))
        })
        .collect();
    let given: BTreeSet<(u16, &str)> = Condition::ALL
        .iter()
        .map(|condition| (condition.status().as_u16(), condition.code()))
        .collect();

    assert_eq!(listed, given);
}

#[test]
fn a_curl_line_is_read_as_curl_and_a_shell_read_it() {
    let block = "$ curl -s -w '\\n%{http_code}\\n' -H \"Authorization: Bearer $token\" \\\n    \
                 -d '{\"path\": \"/home/you/x\"}' \"$api/switch\" | jq '.paths[\"/a b\"][0]'\n\
                 {}\n";
    let shown = commands(block);
    let variables = [("api", "http://127.0.0.1:1/api/v1"), ("token", "t0")];

    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0].printed, "{}\n");
    assert_eq!(
        curl(&words(&shown[0].command, &variables)),
        Curl {
            method: String::from("POST"),
            url: String::from("http://127.0.0.1:1/api/v1/switch"),
            headers: vec![String::from("Authorization: Bearer t0")],
            data: Some(String::from("{\"path\": \"/home/you/x\"}")),
            write_out: Some(String::from("\\n%{http_code}\\n")),
            streamed: false,
            jq: Some(String::from(".paths[\"/a b\"][0]")),
        }
    );
}

#[test]
fn a_jq_path_picks_what_jq_prints() {
    let value: Value = serde_json::from_str(
        r#"{"a": {"b": [1, {"c": true}]}, "list": [{"entry": "api"}, {"entry": "web"}],
            "paths": {"/p": {"get": 2}}}"#,
    )
    .unwrap();

    assert_eq!(jq(".", &value), std::slice::from_ref(&value));
    assert_eq!(jq(".a.b[1].c", &value), [Value::Bool(true)]);
    assert_eq!(jq(".list[].entry", &value), ["api", "web"]);
    assert_eq!(jq(".paths[\"/p\"].get", &value), [2]);
    assert_eq!(jq(".missing", &value), [Value::Null]);
}
