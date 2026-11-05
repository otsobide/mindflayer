//! The CLI's own mothers, beside core's (`mindweaver_core::mother`), which
//! make the domain's values: what a suite types into a command where it
//! takes free text, and a workspace left as an older build left it.
//!
//! Random, as core's are, so a run tries values no test spells: a test
//! drawing from one names the value in what it asserts.

use std::path::Path;

use mindweaver_core::mcp::Given;
use mindweaver_core::mother::random;
use mindweaver_core::WeaverWorkspace;

/// Makes the values a person or a browser types where a command takes free
/// text: a description, a template's name, a reference.
pub struct ValueMother;

impl ValueMother {
    /// What leads a value now and then: the shapes of an option, alone or
    /// given a value of its own, which a line must still pass on as text.
    const LEADS: [&'static str; 9] = [
        "",
        "",
        "",
        "-",
        "--",
        "--yes",
        "--yes=",
        "-p ",
        "--template=",
    ];

    /// Text of any script, with a space, a quote or an `=` in it as often
    /// as not, and led by the shape of an option as often as not.
    ///
    /// Never blank, which a command refuses for what it is rather than for
    /// how it was passed, and never holding `://`, which with an `@` after
    /// it reads as a URL's credential, taken out of a line where it is
    /// shown: what is held to the line as shown would differ for that
    /// alone.
    pub fn random() -> String {
        loop {
            let value = format!(
                "{}{}",
                random::pick(&Self::LEADS),
                random::text(random::between(1, 24))
            );
            if !value.trim().is_empty() && !value.contains("://") {
                return value;
            }
        }
    }

    /// One that starts with a dash, as an option does.
    pub fn dashed() -> String {
        loop {
            let value = Self::random();
            if value.starts_with('-') {
                return value;
            }
        }
    }
}

/// Makes a workspace hold what a Mindweaver from before a URL's credentials
/// were refused left in it.
pub struct OlderBuildMother;

impl OlderBuildMother {
    /// Leave in the workspace at `dir` what such a build left having
    /// gathered its sources from `url` and installed from them: the URL in
    /// every source, in the origin of every installation from one, in an
    /// entry of the log, and in each clone's config and reflog.
    ///
    /// Written again over what an earlier call left, so a suite trying
    /// every form of URL in one workspace finds only the last stored.
    pub fn stored(dir: &Path, url: &str) {
        let workspace = WeaverWorkspace::open(dir).unwrap();
        let database = rusqlite::Connection::open(workspace.ledger_path()).unwrap();
        database
            .execute("UPDATE sources SET url = ?1", [url])
            .unwrap();
        database
            .execute(
                "UPDATE installations SET origin = ?1 WHERE source_id IS NOT NULL",
                [url],
            )
            .unwrap();
        database
            .execute(
                "INSERT INTO actions (at, action, target, outcome, detail)
                 VALUES (0, 'gather', ?1, 'ok', '1 added, 0 updated, 0 unchanged, 0 failed')",
                [url],
            )
            .unwrap();

        let Ok(clones) = std::fs::read_dir(workspace.cache_dir()) else {
            return;
        };
        for clone in clones {
            let git = clone.unwrap().path().join(".git");
            let config: String = std::fs::read_to_string(git.join("config"))
                .unwrap()
                .lines()
                .map(|line| match line.trim_start().strip_prefix("url = ") {
                    Some(_) => format!("\turl = {url}\n"),
                    None => format!("{line}\n"),
                })
                .collect();
            std::fs::write(git.join("config"), config).unwrap();
            let reflog = git.join("logs").join("HEAD");
            let mut kept = std::fs::read_to_string(&reflog).unwrap_or_default();
            kept.push_str(&format!(
                "0000 1111 Fixture <fixture@example.com> 0 +0000\tclone: from {url}\n"
            ));
            std::fs::write(reflog, kept).unwrap();
        }
    }

    /// Everything the workspace at `dir` keeps that could hold a URL: its
    /// database, byte for byte, and every text file in its cache.
    pub fn kept(dir: &Path) -> String {
        fn text_under(root: &Path, into: &mut String) {
            // No cache yet is nothing kept in one.
            let Ok(entries) = std::fs::read_dir(root) else {
                return;
            };
            for entry in entries {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    text_under(&path, into);
                } else if let Ok(text) = std::fs::read_to_string(&path) {
                    into.push_str(&text);
                }
            }
        }
        let workspace = WeaverWorkspace::open(dir).unwrap();
        let mut kept =
            String::from_utf8_lossy(&std::fs::read(workspace.ledger_path()).unwrap()).into_owned();
        text_under(&workspace.cache_dir(), &mut kept);
        kept
    }
}

/// Makes what a Mindflayer build printed for a shell: its activation, under
/// the names it set (D-26).
pub struct MindflayerShellMother;

impl MindflayerShellMother {
    /// The bash or zsh code such a build printed where this one prints
    /// `code`: the same code, setting `$MINDFLAYER_WORKSPACE` and defining
    /// `flayer-deactivate`, and with no older activation of its own to undo
    /// first. Made from this build's code rather than spelled here, so it
    /// is what that build printed, the prompt and the virtualenv's handling
    /// included.
    pub fn activation(code: &str) -> String {
        let undo_older =
            "if typeset -f flayer-deactivate >/dev/null 2>&1; then\n    flayer-deactivate\nfi\n";
        code.strip_prefix(undo_older)
            .expect("the code undoes an older activation first")
            .replace("MINDWEAVER_WORKSPACE", "MINDFLAYER_WORKSPACE")
            .replace("mindweaver-deactivate", "flayer-deactivate")
    }
}

/// Makes the options `add mcp` is told a server's transport by: those a
/// command line gives what core's `McpMother` makes by, and those no
/// definition can hold together, which the grammar refuses.
pub struct ServerOptionsMother;

impl ServerOptionsMother {
    /// The options a command line gives `given` by, in the order the
    /// grammar lists them, each value joined to its option by `=`, so one
    /// led by a dash reads as the value it is.
    pub fn words(given: &Given) -> Vec<String> {
        let mut words = Vec::new();
        let mut give = |option: &str, value: &str| words.push(format!("--{option}={value}"));
        match given {
            Given::Program(program) => {
                give("command", &program.command);
                for arg in &program.args {
                    give("arg", arg);
                }
                for (key, value) in &program.env {
                    give("env", &format!("{key}={value}"));
                }
                for variable in &program.secret_env {
                    give("secret-env", variable);
                }
                for variable in &program.arg_env {
                    give("arg-env", variable);
                }
            }
            Given::Address(address) => {
                give("url", &address.url);
                for (key, value) in &address.headers {
                    give("header", &format!("{key}={value}"));
                }
                for (header, variable) in &address.secret_headers {
                    give("secret-header", &format!("{header}={variable}"));
                }
                if let Some(variable) = &address.bearer {
                    give("bearer-env", variable);
                }
                for (name, variable) in &address.url_vars {
                    give("url-var", &format!("{name}={variable}"));
                }
                if address.events {
                    words.push(String::from("--sse"));
                }
            }
        }
        words
    }

    /// Options no definition can hold together, each beside why: two
    /// transports, an option of one with the other, or with neither, and a
    /// transport, or any option of one, with a template, which says how a
    /// server is started as surely.
    pub fn conflicting() -> Vec<(&'static str, Vec<&'static str>)> {
        vec![
            (
                "a program and an address",
                vec!["--command", "npx", "--url", "https://example.com/mcp"],
            ),
            (
                "a header for a program",
                vec!["--command", "npx", "--header", "X-Client=me"],
            ),
            (
                "a secret header for a program",
                vec!["--command", "npx", "--secret-header", "X-Api-Key=KEY"],
            ),
            ("events for a program", vec!["--command", "npx", "--sse"]),
            (
                "an argument for an address",
                vec!["--url", "https://example.com/mcp", "--arg", "-y"],
            ),
            (
                "a secret in the environment of an address",
                vec!["--url", "https://example.com/mcp", "--secret-env", "KEY"],
            ),
            ("an argument with nothing to start", vec!["--arg", "-y"]),
            ("events with no address", vec!["--sse"]),
            (
                "a template and a program",
                vec!["--template", "internal", "--command", "npx"],
            ),
            (
                "a template and an address",
                vec!["--template", "internal", "--url", "https://example.com/mcp"],
            ),
            // Each of these requires a transport that conflicts with the
            // template, a requirement clap would otherwise pass over.
            (
                "a template and an argument",
                vec!["--template", "internal", "--arg", "-y"],
            ),
            (
                "a template and a setting",
                vec!["--template", "internal", "--env", "LOG_LEVEL=info"],
            ),
            (
                "a template and a secret in the environment",
                vec!["--template", "internal", "--secret-env", "KEY"],
            ),
            (
                "a template and an argument read from a variable",
                vec!["--template", "internal", "--arg-env", "KEY"],
            ),
            (
                "a template and events",
                vec!["--template", "internal", "--sse"],
            ),
            (
                "a template and a header",
                vec!["--template", "internal", "--header", "X-Client=me"],
            ),
            (
                "a template and a secret header",
                vec!["--template", "internal", "--secret-header", "X-Api-Key=KEY"],
            ),
            (
                "a template and a bearer token",
                vec!["--template", "internal", "--bearer-env", "KEY"],
            ),
            (
                "a template and a part of the address",
                vec!["--template", "internal", "--url-var", "tenant=TENANT"],
            ),
            (
                "a template and options of both transports",
                vec![
                    "--template",
                    "internal",
                    "--secret-env",
                    "KEY",
                    "--bearer-env",
                    "OTHER",
                ],
            ),
            (
                "a setting with no `=`",
                vec!["--command", "npx", "--env", "KEY"],
            ),
        ]
    }
}
