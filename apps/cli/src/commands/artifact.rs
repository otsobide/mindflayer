//! The commands that change artifacts: `add`, `edit`, `rename`, `remove` and
//! `templates`, at either level.
//!
//! Each takes the holder it acts on — a project, or the workspace's own — so
//! `mind add` and `flayer add -p alpha` are one function given one project,
//! and `flayer add` is the same function given the workspace. Each also takes
//! the workspace the command resolved, which the change is told about: its
//! record of what it installed there must not outlive the copy.

use std::fmt::Write as _;
use std::path::Path;

use mindflayer_core::template::{self, Origin};
use mindflayer_core::{
    create, create_from, lifecycle, paths, FlayerWorkspace, Kind, MindProject, Reference, Target,
};

use crate::render::{plural, printable, table};
use crate::{CliError, Failure, Outcome};

pub(crate) fn add_to(
    workspace: Option<&FlayerWorkspace>,
    holder: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    template: Option<&str>,
) -> Result<Outcome, Failure> {
    let artifact = match template {
        None => create(holder, kind, name, description),
        Some(template) => {
            let template = template::find(holder, kind, template).map_err(CliError::from)?;
            create_from(holder, kind, name, description, &template)
        }
    }
    .map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "created {} {} at {}\n",
        artifact.kind(),
        artifact.name(),
        artifact.file().display()
    ));
    disowned(
        &mut outcome,
        workspace,
        holder,
        kind,
        name,
        "an old record said `flayer install` had put one of that name here; it is gone",
    );
    Ok(outcome)
}

pub(crate) fn edit_in(
    workspace: Option<&FlayerWorkspace>,
    holder: &MindProject,
    reference: &str,
    editor: Option<&str>,
) -> Result<Outcome, Failure> {
    let target = lifecycle::find(holder, &Reference::parse(reference)).map_err(CliError::from)?;
    let file = target.file();
    let before = std::fs::read(&file).ok();
    let editor = editor.map_or_else(default_editor, str::to_owned);
    open_in_editor(&editor, &file)?;
    let changed = std::fs::read(&file).ok() != before;

    let mut outcome = edited(holder, &target, changed);
    if changed {
        disowned(
            &mut outcome,
            workspace,
            holder,
            target.kind,
            &target.name,
            "it had been installed from the shelf; edited, it is yours, and `flayer install` leaves it alone",
        );
    }
    Ok(outcome)
}

pub(crate) fn rename_in(
    workspace: Option<&FlayerWorkspace>,
    holder: &MindProject,
    reference: &str,
    to: &str,
) -> Result<Outcome, Failure> {
    let target = lifecycle::find(holder, &Reference::parse(reference)).map_err(CliError::from)?;
    let renamed = lifecycle::rename(holder, &target, to).map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "renamed {} {} to {} at {}\n",
        target.kind,
        target.name,
        renamed.name,
        renamed.path.display()
    ));
    disowned(
        &mut outcome,
        workspace,
        holder,
        target.kind,
        &target.name,
        "it had been installed from the shelf; renamed, it is yours, and `flayer install` leaves it alone",
    );
    // A stale claim on the new name would make what was just renamed look
    // like something Mindflayer put there.
    disowned(
        &mut outcome,
        workspace,
        holder,
        target.kind,
        &renamed.name,
        "an old record said `flayer install` had put one of that name here; it is gone",
    );
    Ok(outcome)
}

pub(crate) fn remove_from(
    workspace: Option<&FlayerWorkspace>,
    holder: &MindProject,
    reference: &str,
    yes: bool,
) -> Result<Outcome, Failure> {
    let target = lifecycle::find(holder, &Reference::parse(reference)).map_err(CliError::from)?;
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
    lifecycle::remove(holder, &target).map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "removed {} {} from {}\n",
        target.kind,
        target.name,
        target.path.display()
    ));
    disowned(
        &mut outcome,
        workspace,
        holder,
        target.kind,
        &target.name,
        "it had been installed from the shelf; `flayer install` can put it back",
    );
    Ok(outcome)
}

pub(crate) fn templates_of(holder: &MindProject) -> Result<Outcome, Failure> {
    let templates = template::templates(holder).map_err(CliError::from)?;
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
/// one every system has — the order git looks in.
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
/// now still passes `validate` — the question worth answering the moment the
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

    let issues = artifact.validate();
    if issues.is_empty() {
        return Outcome::plain(format!("{head}: ok\n"));
    }
    let mut stdout = format!("{head}: {}\n", plural(issues.len(), "problem"));
    for issue in issues {
        let _ = writeln!(stdout, "  - {issue}");
    }
    Outcome {
        stdout,
        stderr: Vec::new(),
        ok: false,
    }
}

/// Drop a record saying `flayer install` put `name` into `project`, and say so
/// when there was one. With no workspace there is no record to drop.
///
/// Failing to is a warning rather than an error: the change the record was
/// about has already been made, and reporting it as not made would be false.
fn disowned(
    outcome: &mut Outcome,
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    kind: Kind,
    name: &str,
    why: &str,
) {
    let Some(workspace) = workspace else {
        return;
    };
    match mindflayer_core::install::disown(workspace, project, kind, name) {
        Ok(true) => {
            let _ = writeln!(outcome.stdout, "  {why}");
        }
        Ok(false) => {}
        Err(error) => {
            outcome.stderr.push(format!("warning: {error}"));
            outcome.ok = false;
        }
    }
}
