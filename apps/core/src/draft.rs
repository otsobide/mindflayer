//! What core says of an artifact a command is about to write, before it
//! writes it: for a front end that judges what is being typed, the panel's
//! forms and its editor (#82).
//!
//! Two things, each core's and said in core's words, so a front end judging
//! a draft holds no rule of its own and says nothing the command would not:
//!
//! - **Why the command would write nothing**, by the checks it makes before
//!   it writes, called rather than copied: what `add` works out before it
//!   makes a folder ([`crate::create`]), and what `edit --contents` holds
//!   contents to before it reads the file ([`lifecycle::replaceable`]). Its
//!   code and its message are the ones the command gives.
//! - **What `validate` would say of it** once written: the artifact read
//!   from what it would hold, as discovery reads one from the disk, and
//!   judged by [`Artifact::validate`], so each issue's message is word for
//!   word what `flayer validate` prints of it then. Nothing, when what
//!   would be written cannot be told: `add` refusing before it builds
//!   anything, or a skill whose front matter does not read, which the
//!   refusal says.
//!
//! Only a read: nothing is written, and the ledger is not opened. What a
//! write alone can tell, whether the file still holds what an edit was made
//! from, or whether a name taken a moment ago is free again, it tells when
//! it writes; a draft judged fit can still be refused then.

use serde::Serialize;

use crate::artifact::{Artifact, ValidationIssue};
use crate::catalog::Reference;
use crate::code::Said;
use crate::create;
use crate::kind::Kind;
use crate::lifecycle;
use crate::template;
use crate::workspace::{FlayerWorkspace, MindProject};

/// What core says of a draft: what `validate` would say of it once written,
/// and why the command writing it would write nothing, when it would.
///
/// Each field by its name, a contract once a front end reads it, as a
/// report's are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Judged {
    /// Everything `validate` would say of the artifact as it would be
    /// written, warnings included, in its words: none when what would be
    /// written cannot be told.
    pub issues: Vec<ValidationIssue>,
    /// Why the command would write nothing, by the code and in the words it
    /// would refuse with; none when it would write.
    pub refused: Option<Said>,
}

impl Judged {
    /// Refused for `error`, with nothing written to judge.
    fn refused<E: std::fmt::Display>(code: &'static str, error: &E) -> Judged {
        Judged {
            issues: Vec::new(),
            refused: Some(Said::of(code, error)),
        }
    }
}

/// What `add` would make of `kind` `name`, described as `description`,
/// started from the template `template` names when it names one, in
/// `project`; `workspace` is the one consulted, whose templates are among
/// those offered, as for [`lifecycle::add`].
///
/// The template found as `add` finds it ([`template::find`]), then every
/// check `add` makes before it writes, and then whether something is where
/// it would go already, which `add` leaves alone and refuses.
pub fn added(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    template: Option<&str>,
) -> Judged {
    let template = match template
        .map(|template| template::find(workspace, project, kind, template))
        .transpose()
    {
        Ok(template) => template,
        Err(error) => return Judged::refused(error.code(), &error),
    };
    let scaffold = match create::plan(project, kind, name, description, template.as_ref()) {
        Ok(scaffold) => scaffold,
        Err(error) => return Judged::refused(error.code(), &error),
    };
    let issues = scaffold
        .read_as(project)
        .map(|artifact| artifact.validate())
        .unwrap_or_default();
    Judged {
        issues,
        refused: scaffold.taken().map(|error| Said::of(error.code(), &error)),
    }
}

/// What `edit --contents` would make of `contents` given for the artifact
/// `reference` names in `project`; `workspace` is the one consulted, as for
/// [`lifecycle::replace_contents`].
///
/// The artifact found as the edit finds it ([`lifecycle::find_to_change`]),
/// then every rule the edit holds contents to before it reads the file
/// ([`lifecycle::replaceable`]), and `validate`'s verdict of the artifact
/// holding them, in the place it is.
pub fn edited(
    workspace: Option<&FlayerWorkspace>,
    project: &MindProject,
    reference: &Reference,
    contents: &str,
) -> Judged {
    let target = match lifecycle::find_to_change(workspace, project, reference) {
        Ok(target) => target,
        Err(error) => return Judged::refused(error.code(), &error),
    };
    let refused = lifecycle::replaceable(workspace, project, &target, contents.as_bytes())
        .err()
        .map(|error| Said::of(error.code(), &error));
    // Matched on the kind, as the edit's own rules are, so a third one has
    // to say how it is read.
    let artifact = match target.kind {
        Kind::Skill => Artifact::skill_holding(&target.path, project.root(), contents).ok(),
        Kind::Rule => Some(Artifact::rule_holding(
            &target.path,
            target.name.clone(),
            project.root(),
            contents,
        )),
    };
    Judged {
        issues: artifact
            .map(|artifact| artifact.validate())
            .unwrap_or_default(),
        refused,
    }
}

#[cfg(test)]
#[path = "draft_test.rs"]
mod tests;
