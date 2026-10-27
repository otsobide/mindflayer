//! The commands that change artifacts: `add`, `edit`, `rename`, `remove` and
//! `templates`, at either level.
//!
//! Each takes the holder it acts on (a project, or the workspace's own), so
//! `mind add` and `flayer add -p alpha` are one function given one project,
//! and `flayer add` is the same function given the workspace. Each also takes
//! the workspace the command resolved and hands it to core: its templates are
//! among those `add` starts from, and core's lifecycle sees to its record of
//! what it installed: kept through an edit, carried through a rename, and
//! dropped when the copy goes. Here that is only said, along with what
//! opening the ledger to do it scrubbed out of it.

use std::fmt::Write as _;
use std::path::Path;

use mindflayer_core::install::InstallError;
use mindflayer_core::lifecycle::{self, Record};
use mindflayer_core::template::{self, Origin};
use mindflayer_core::{paths, FlayerWorkspace, Kind, MindProject, Reference, Target};

use crate::commands::gather::scrubbed;
use crate::render::{printable, table, verdict};
use crate::{CliError, Failure, Outcome};

pub(crate) fn add_to(
    workspace: Option<&FlayerWorkspace>,
    holder: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    template: Option<&str>,
) -> Result<Outcome, Failure> {
    let template = template
        .map(|template| template::find(workspace, holder, kind, template))
        .transpose()
        .map_err(CliError::from)?;
    let added = lifecycle::add(
        workspace,
        holder,
        kind,
        name,
        description,
        template.as_ref(),
    )
    .map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "created {} {} at {}\n",
        added.artifact.kind(),
        added.artifact.name(),
        added.artifact.file().display()
    ));
    recorded(&mut outcome, added.stale, stale);
    Ok(scrubbed(&added.scrub).before(outcome))
}

pub(crate) fn edit_in(
    workspace: Option<&FlayerWorkspace>,
    holder: &MindProject,
    reference: &str,
    editor: Option<&str>,
) -> Result<Outcome, Failure> {
    let target = lifecycle::find_to_change(workspace, holder, &Reference::parse(reference))
        .map_err(CliError::from)?;
    let edit = lifecycle::start_edit(&target);
    let editor = editor.map_or_else(default_editor, str::to_owned);
    let opened = open_in_editor(&editor, edit.file());
    // Finished whatever the editor came to. One that writes the file and then
    // exits non-zero, as vim does on `:cq`, has changed the copy all the same.
    let finished = lifecycle::finish_edit(workspace, holder, edit);
    if let Err(error) = opened {
        let mut failure = Failure::from(error);
        if let Err(error) = finished.copy {
            failure.warnings.push(format!("warning: {error}"));
        }
        return Err(failure);
    }

    let mut outcome = edited(holder, &target, finished.changed);
    recorded(&mut outcome, finished.copy, |record| {
        (record == Record::Kept).then(|| {
            String::from(
                "it was installed by `flayer install`, and stays so; edited, it is left alone by \
                 `flayer install` and `flayer uninstall` unless they are forced",
            )
        })
    });
    Ok(outcome)
}

pub(crate) fn rename_in(
    workspace: Option<&FlayerWorkspace>,
    holder: &MindProject,
    reference: &str,
    to: &str,
) -> Result<Outcome, Failure> {
    let target = lifecycle::find_to_change(workspace, holder, &Reference::parse(reference))
        .map_err(CliError::from)?;
    let renamed = lifecycle::rename(workspace, holder, &target, to).map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "renamed {} {} to {} at {}\n",
        target.kind,
        target.name,
        renamed.target.name,
        renamed.target.path.display()
    ));
    let to = renamed.target.name.clone();
    recorded(&mut outcome, renamed.copy, |record| match record {
        Record::Moved { edited: true } => Some(String::from(
            "it was installed by `flayer install`, and stays so under its new name; renamed, it \
             is left alone by `flayer install` and `flayer uninstall` unless they are forced",
        )),
        // A rule's rename moves its file as it is, so nothing in it reads
        // as an edit, and the uninstall that put it there takes it out.
        Record::Moved { edited: false } => Some(String::from(
            "it was installed by `flayer install`, and stays so under its new name; renamed, it \
             still holds what was installed, and `flayer uninstall` takes it out",
        )),
        Record::Dropped => Some(format!(
            "it had been installed, but `{to}` is installed already; renamed, it is yours, and \
             `flayer install` leaves it alone"
        )),
        Record::Untouched | Record::Kept => None,
    });
    recorded(&mut outcome, renamed.stale, stale);
    Ok(scrubbed(&renamed.scrub).before(outcome))
}

pub(crate) fn remove_from(
    workspace: Option<&FlayerWorkspace>,
    holder: &MindProject,
    reference: &str,
    yes: bool,
) -> Result<Outcome, Failure> {
    let target = lifecycle::find_to_change(workspace, holder, &Reference::parse(reference))
        .map_err(CliError::from)?;
    // Deleting is the one thing here that cannot be taken back, so it is
    // asked for twice: once by naming it, once by saying yes.
    if !yes {
        return Err(CliError::Unconfirmed {
            kind: target.kind,
            path: target.path.clone(),
            files: lifecycle::files(&target),
        }
        .into());
    }
    let removed = lifecycle::remove(workspace, holder, &target).map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "removed {} {} from {}\n",
        target.kind,
        target.name,
        target.path.display()
    ));
    recorded(&mut outcome, removed.copy, |record| {
        (record == Record::Dropped).then(|| {
            String::from(
                "it had been installed, and is uninstalled with it; `flayer install` can put it back",
            )
        })
    });
    Ok(scrubbed(&removed.scrub).before(outcome))
}

pub(crate) fn templates_of(
    workspace: Option<&FlayerWorkspace>,
    holder: &MindProject,
) -> Result<Outcome, Failure> {
    let templates = template::templates(workspace, holder).map_err(CliError::from)?;
    let rows: Vec<Vec<String>> = templates
        .iter()
        .map(|template| {
            let origin = match &template.origin {
                Origin::BuiltIn => String::from("built in"),
                Origin::Path(path) => paths::relative_to(path, holder.root())
                    .as_deref()
                    .and_then(paths::to_config_string)
                    .unwrap_or_else(|| path.display().to_string()),
            };
            vec![
                printable(&template.name),
                template.kind.slug().to_owned(),
                printable(&origin),
            ]
        })
        .collect();
    Ok(Outcome::plain(table(&rows)))
}

// ---------------------------------------------------------------------------
// edit, rename, remove
// ---------------------------------------------------------------------------

/// The editor to use when none was named: `$VISUAL`, then `$EDITOR`, then the
/// one every system has: the order git looks in.
fn default_editor() -> String {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.trim().is_empty())
        .unwrap_or_else(|| String::from(if cfg!(windows) { "notepad" } else { "vi" }))
}

/// Open `file` in `editor` and wait for it to be closed.
///
/// On Unix the editor runs through `sh`, as git runs it, so `code --wait` and
/// an editor whose path has a space in it both work. The file is passed as an
/// argument rather than spliced into the command, so its name is never read
/// by the shell.
fn open_in_editor(editor: &str, file: &Path) -> Result<(), CliError> {
    #[cfg(unix)]
    let mut command = {
        let mut command = std::process::Command::new("sh");
        command
            .arg("-c")
            .arg(format!("{editor} \"$1\""))
            .arg(editor)
            .arg(file);
        command
    };
    #[cfg(not(unix))]
    let mut command = {
        let mut words = editor.split_whitespace();
        let mut command = std::process::Command::new(words.next().unwrap_or("notepad"));
        command.args(words).arg(file);
        command
    };

    let status = command.status().map_err(|source| CliError::Editor {
        editor: editor.to_owned(),
        source,
    })?;
    if !status.success() {
        return Err(CliError::EditorFailed {
            editor: editor.to_owned(),
            status: status.to_string(),
        });
    }
    Ok(())
}

/// What editing came to: whether anything changed, and whether what is there
/// now still passes `validate`, the question worth answering the moment the
/// editor closes, not at the next CI run.
fn edited(project: &MindProject, target: &Target, changed: bool) -> Outcome {
    let what = format!("{} {}", target.kind, target.name);
    let head = if changed {
        format!("edited {what}")
    } else {
        format!("left {what} unchanged")
    };
    let artifact = match lifecycle::load(project, target.kind, &target.path, &target.name) {
        Ok(artifact) => artifact,
        Err(error) => {
            return Outcome {
                stdout: format!("{head}, and it does not read\n"),
                stderr: vec![format!("warning: {error}")],
                ok: false,
            }
        }
    };

    let mut stdout = String::new();
    let invalid = verdict(&mut stdout, &head, &artifact.validate());
    Outcome {
        stdout,
        stderr: Vec::new(),
        ok: !invalid,
    }
}

/// Say what a change did to a record that `flayer install` put something
/// into the project, when `why` has something to say of it.
///
/// A record that could not be changed is a warning rather than an error, as
/// core reports it: the change it was about has already been made, and
/// reporting it as not made would be false.
fn recorded(
    outcome: &mut Outcome,
    record: Result<Record, InstallError>,
    why: impl Fn(Record) -> Option<String>,
) {
    match record {
        Ok(record) => {
            if let Some(why) = why(record) {
                let _ = writeln!(outcome.stdout, "  {why}");
            }
        }
        Err(error) => {
            outcome.stderr.push(format!("warning: {error}"));
            outcome.ok = false;
        }
    }
}

/// What to say of a record of an install under the name a change wrote,
/// whose copy had gone, and which the change dropped.
fn stale(record: Record) -> Option<String> {
    (record == Record::Dropped).then(|| {
        String::from(
            "an old record said `flayer install` had put one of that name here; it is gone",
        )
    })
}
