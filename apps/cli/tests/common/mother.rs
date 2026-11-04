//! The CLI's own mothers, beside core's (`mindflayer_core::mother`), which
//! make the domain's values: what a suite types into a command where it
//! takes free text, and a workspace left as an older build left it.
//!
//! Random, as core's are, so a run tries values no test spells: a test
//! drawing from one names the value in what it asserts.

use std::path::Path;

use mindflayer_core::mother::random;
use mindflayer_core::FlayerWorkspace;

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

/// Makes a workspace hold what a Mindflayer from before a URL's credentials
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
        let workspace = FlayerWorkspace::open(dir).unwrap();
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
        let workspace = FlayerWorkspace::open(dir).unwrap();
        let mut kept =
            String::from_utf8_lossy(&std::fs::read(workspace.ledger_path()).unwrap()).into_owned();
        text_under(&workspace.cache_dir(), &mut kept);
        kept
    }
}
