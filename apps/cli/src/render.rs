//! What a command produced, and the helpers every report is laid out with.
//!
//! Apart from the commands so that a report is built one way: a row is made
//! `printable` and padded by `table` whichever command or screen it comes
//! from, and a new command cannot quietly grow a second way of doing either.

use std::fmt::Write as _;
use std::io;
use std::path::Path;
use std::process::ExitCode;

use mindflayer_core::{paths, ArtifactError, ResolveError, ValidationIssue, WorkspaceError};
use serde::Serialize;

use crate::CliError;

/// What a command produced.
///
/// The text is built rather than printed as the command runs, which is what
/// lets a test assert on exactly what a user sees.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The report itself.
    pub stdout: String,
    /// Problems worth reporting that did not stop the command.
    pub stderr: Vec<String>,
    /// Whether everything the command looked at was in order.
    pub ok: bool,
}

impl Outcome {
    /// A clean report with nothing to warn about.
    pub(crate) fn plain(stdout: String) -> Self {
        Self {
            stdout,
            stderr: Vec::new(),
            ok: true,
        }
    }

    /// Print the outcome and return the code the process should exit with.
    ///
    /// Written rather than printed, because `print!` panics when the pipe is
    /// closed and `mind list | head` closes it on purpose. A reader that has
    /// seen enough is not an error, so that case exits as if all was well.
    pub fn report(&self) -> ExitCode {
        use std::io::Write as _;

        if let Err(error) = io::stdout().write_all(self.stdout.as_bytes()) {
            return if error.kind() == io::ErrorKind::BrokenPipe {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        for line in &self.stderr {
            let _ = writeln!(io::stderr(), "{line}");
        }
        if self.ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}

/// A command that could not run, and whatever it had already found out.
///
/// The warnings travel with the error rather than being dropped, because they
/// usually explain it: `no skill named \`broken\`` is baffling on its own and
/// obvious next to `broken/SKILL.md: the file does not start with a `---`
/// front matter fence`.
#[derive(Debug)]
pub struct Failure {
    /// Why the command stopped.
    ///
    /// Boxed to keep the error half of every `Result` small: a `CliError`
    /// carries paths and an io::Error, and the success path should not pay for
    /// that on every return.
    pub error: Box<CliError>,
    /// What it had already noticed before it stopped.
    pub warnings: Vec<String>,
}

impl Failure {
    /// Print the warnings and the error, and return the process exit code.
    pub fn report(&self) -> ExitCode {
        for line in &self.warnings {
            eprintln!("{line}");
        }
        eprintln!("error: {}", self.error);
        ExitCode::FAILURE
    }
}

impl From<CliError> for Failure {
    fn from(error: CliError) -> Self {
        Self {
            error: Box::new(error),
            warnings: Vec::new(),
        }
    }
}

impl From<WorkspaceError> for Failure {
    fn from(error: WorkspaceError) -> Self {
        CliError::from(error).into()
    }
}

impl From<ResolveError> for Failure {
    fn from(error: ResolveError) -> Self {
        CliError::from(error).into()
    }
}

impl From<ArtifactError> for Failure {
    fn from(error: ArtifactError) -> Self {
        CliError::from(error).into()
    }
}

/// The version of what every reading command's `--json` prints around what
/// it read ([`json`]).
///
/// It moves when a field of the envelope goes or changes what it holds, and
/// not when one is added, which a reader passes over. What a command puts in
/// `data` is held to the same rule: every name in it is a contract, which is
/// why core spells each kind, agent and state as a command line takes it.
pub const ENVELOPE_VERSION: u32 = 1;

/// What every reading command's `--json` prints:
/// `{"version": 1, "data": ..., "warnings": [...]}`.
///
/// One shape for every command, so a script reads each the same way, and the
/// warnings inside it beside the data rather than on stderr alone: failures
/// are collected, not raised, in JSON as in text.
#[derive(Serialize)]
struct Envelope<'a, T: Serialize> {
    version: u32,
    data: &'a T,
    warnings: Vec<String>,
}

/// `data` in the envelope, with what the command would have said on stderr
/// as `warning:` lines in its `warnings`, without the prefix, and the exit
/// status the text would have had.
///
/// Pretty-printed, two spaces a level, since a person reads it as often as a
/// script does, and `jq` takes either.
pub(crate) fn json<T: Serialize>(data: &T, warnings: Vec<String>) -> Outcome {
    let ok = warnings.is_empty();
    let envelope = Envelope {
        version: ENVELOPE_VERSION,
        data,
        warnings,
    };
    // A report is strings, numbers and lists of them, and maps keyed by
    // field names, and every path in one is serialized as text whatever
    // bytes it holds: nothing in it can fail to serialize.
    let mut stdout = serde_json::to_string_pretty(&envelope).expect("a report serializes");
    stdout.push('\n');
    Outcome {
        stdout,
        stderr: Vec::new(),
        ok,
    }
}

/// Rows padded into columns, the last one left ragged.
pub(crate) fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.first().map_or(0, Vec::len);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut text = String::new();
    for row in rows {
        let mut line = String::new();
        for (column, cell) in row.iter().enumerate() {
            if column + 1 == columns {
                line.push_str(cell);
            } else {
                let padding = widths[column].saturating_sub(cell.chars().count());
                let _ = write!(line, "{cell}{}  ", " ".repeat(padding));
            }
        }
        // An artifact with no summary would otherwise leave the padding of the
        // column before it hanging off the end of the line.
        let _ = writeln!(text, "{}", line.trim_end());
    }
    text
}

/// Two spaces in front of every line, so a block reads as belonging to the
/// line above it.
pub(crate) fn indent(text: &str) -> String {
    text.lines().fold(String::new(), |mut out, line| {
        let _ = writeln!(out, "  {line}");
        out
    })
}

/// Text safe to put in a row.
///
/// A name or an opening line comes from a file somebody wrote, and a carriage
/// return in one would let a row overwrite the row above it on the terminal,
/// hiding an entry, or fabricating one that looks real.
pub(crate) fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// What is wrong with one artifact: `label: ok`, or the count after the
/// label and a line for each thing under it. Whether the artifact is invalid
/// is returned, so a report can count it and an exit status follow it.
///
/// Only a problem makes it invalid. A warning ([`ValidationIssue::is_warning`])
/// is listed, marked as one, and leaves the verdict at `ok`.
pub(crate) fn verdict(text: &mut String, label: &str, issues: &[ValidationIssue]) -> bool {
    let warnings = issues.iter().filter(|issue| issue.is_warning()).count();
    let problems = issues.len() - warnings;
    let mut counts = Vec::new();
    if problems == 0 {
        counts.push(String::from("ok"));
    } else {
        counts.push(plural(problems, "problem"));
    }
    if warnings > 0 {
        counts.push(plural(warnings, "warning"));
    }
    let _ = writeln!(text, "{label}: {}", counts.join(", "));
    // Printable, because an issue quotes the name or the key it is about.
    for issue in issues {
        let marked = if issue.is_warning() { "warning: " } else { "" };
        let _ = writeln!(text, "  - {marked}{}", printable(&issue.to_string()));
    }
    problems > 0
}

/// "1 skill", "2 skills".
pub(crate) fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// A registry entry spelled the way the marker file spells it.
///
/// Not `Path::display`, which uses the platform separator: entries are stored
/// with forward slashes so a workspace registered on one platform resolves on
/// the other, and on Windows `display` would report `..\collapse` for a line
/// that reads `../collapse`. What a command says it wrote has to be what
/// someone opening the file will find.
pub(crate) fn as_stored(entry: &Path) -> String {
    paths::to_config_string(entry).unwrap_or_else(|| entry.display().to_string())
}
