//! How what a project was given stands against what was installed (D-04).
//!
//! The ledger keeps, for every copy an installation wrote, a hash of what it
//! wrote: once for a skill's installation, since every copy is staged from
//! one tree, and once a copy for a rule's, whose section in `AGENTS.md` and
//! file for Claude hold different bytes. A copy that still holds that hash
//! is untouched, and Mindflayer may write over it or take it out; one that
//! holds anything else was edited, somebody's work from then on, refused to
//! every install and uninstall but one that is forced. Neither is told by
//! the copy alone, nor by the offer alone: an edited copy can match what is
//! offered now, and an untouched one can be behind it.
//!
//! A record from before hashes were kept has none to hold its copies to. It
//! is held to its offer instead, byte for byte, and counts as current only
//! while it matches, and as edited otherwise: the safe reading, since an
//! untouched copy whose offer has moved on reads the same as an edit.

use std::fs;
use std::path::{Path, PathBuf};

use super::{left_out_names, recorded_copy, Offer};
use crate::agents_md::{self, AgentsMd, Section};
use crate::artifact::Artifact;
use crate::copy;
use crate::hash;
use crate::kind::Kind;
use crate::ledger::{InstallRecord, Target};
use crate::workspace::{FlayerWorkspace, MindProject};

/// How one offer stands with respect to one project.
///
/// The five states between `Absent` and `Foreign` are an installation's, one
/// the ledger records: told apart by the hash of what was written, and by
/// what the place it came from offers now. When more than one is true,
/// edited is said first, then orphaned, missing and outdated: an edit is what
/// a change must not undo, an offer gone leaves nothing to put back or bring
/// up to date, and a copy put back is put back as the offer holds it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// The project does not hold anything by that name, and the ledger
    /// records no installation of it.
    Absent,
    /// Every copy holds what was installed, and the place it came from
    /// offers that still.
    Current,
    /// Every copy holds what was installed, and the place it came from
    /// offers something else by now: an install brings it up to date.
    Outdated,
    /// A copy holds something other than what was installed: somebody's
    /// work, written over or taken out by nothing but a forced change.
    Edited,
    /// A copy the ledger records is not where it was put any more: an
    /// install puts it back.
    Missing,
    /// The place it came from no longer offers it.
    Orphaned,
    /// The project holds something by that name that Mindflayer did not put
    /// there. It is somebody's work, so it is left alone in both directions.
    Foreign,
}

impl Standing {
    /// Whether the project holds it, which is what a checkbox shows:
    /// anything it holds by that name, but not an installation a copy of
    /// which has gone. Its box opens empty, for marking it to put every copy
    /// back, which a `git clean -dX` in a local project makes the common way
    /// back; held, it could only be marked to be taken out. One with a copy
    /// left is taken out with `flayer uninstall`.
    pub fn present(self) -> bool {
        !matches!(self, Standing::Absent | Standing::Missing)
    }

    /// Whether Mindflayer may write over it or remove it without being
    /// forced: never over somebody's, nor over a copy somebody edited.
    pub fn ours(self) -> bool {
        !matches!(self, Standing::Foreign | Standing::Edited)
    }
}

/// How one copy an installation recorded is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Found {
    /// Nothing is at its place.
    Missing,
    /// Something other than what was written there is.
    Edited,
    /// What was written there is.
    Untouched,
}

/// One copy an installation recorded: where it is, and how it is found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Looked {
    pub path: PathBuf,
    pub found: Found,
}

/// One installation the ledger records, as it is found: what
/// [`super::standings`] says of each, kept whole for what acts on it, as an
/// update does ([`crate::update`]).
#[derive(Debug, Clone)]
pub(crate) struct Surveyed {
    pub record: InstallRecord,
    /// Where what it installed is offered now ([`super::offered_for`]).
    pub offered: Vec<Offer>,
    /// Each copy it records, as it is found.
    pub copies: Vec<Looked>,
    pub standing: Standing,
}

impl Surveyed {
    /// Where each of its copies found as `found` is.
    pub(crate) fn found(&self, found: Found) -> Vec<PathBuf> {
        self.copies
            .iter()
            .filter(|copy| copy.found == found)
            .map(|copy| copy.path.clone())
            .collect()
    }

    /// The place it is offered from now that an install would bring it up
    /// to date from: none when nothing offers it, and every place that
    /// does, by its origin, when that cannot be told.
    ///
    /// One place, but for a record from before origins were kept, which
    /// may have come from the workspace's own or from any workspace it
    /// loads ([`super::offered_for`]). Then the one holding what an
    /// untouched copy holds is the one it came from; with no such copy to
    /// tell by, or a rule, whose records always had an origin, it is not
    /// guessed: which of two goes into the project's one folder of that
    /// name is not something to guess, as an install with no `--from` says.
    pub(crate) fn source(
        &self,
        workspace: &FlayerWorkspace,
    ) -> Result<Option<&Offer>, Vec<String>> {
        match &self.offered[..] {
            [] => Ok(None),
            [one] => Ok(Some(one)),
            several => {
                let untouched = self.found(Found::Untouched);
                several
                    .iter()
                    .find(|offer| {
                        self.record.kind == Kind::Skill
                            && untouched
                                .first()
                                .is_some_and(|copy| holds(workspace, offer, copy))
                    })
                    .map(Some)
                    .ok_or_else(|| several.iter().map(Offer::origin).collect())
            }
        }
    }
}

/// Each copy of `record`, a skill's in `project`, as it is found: by the
/// hash its installation recorded, or, for a record with none, against
/// `offered`, where what it installed is offered now ([`super::offered_for`]).
///
/// A route the record holds that is no folder of the project named after
/// the skill is never looked at ([`recorded_copy`]), and counts as missing:
/// nothing of Mindflayer's can be found there.
pub(crate) fn skill_copies(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    record: &InstallRecord,
    offered: &[&Offer],
) -> Vec<Looked> {
    record
        .targets
        .iter()
        .map(|target| {
            let Ok(path) = recorded_copy(project, Kind::Skill, &record.name, &target.path) else {
                return missing(project, target);
            };
            if fs::symlink_metadata(&path).is_err() {
                return Looked {
                    path,
                    found: Found::Missing,
                };
            }
            // Hashed even with no hash to hold it to, so a copy holding a
            // link is never compared through it: the hash refuses one, and
            // an install never writes one.
            let found = match (hash::tree(&path), &record.content_hash) {
                (Ok(held), Some(written)) if held == *written => Found::Untouched,
                (Ok(_), None) if offered.iter().any(|offer| holds(workspace, offer, &path)) => {
                    Found::Untouched
                }
                _ => Found::Edited,
            };
            Looked { path, found }
        })
        .collect()
}

/// How the installation `record`, a skill's in `project`, stands, given
/// `offered`, where what it installed is offered now.
pub(crate) fn of_skill(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    record: &InstallRecord,
    offered: &[&Offer],
) -> Standing {
    let looked = skill_copies(workspace, project, record, offered);
    skill_standing(workspace, &looked, offered)
}

/// How a skill's installation stands from how its copies were `looked` at
/// ([`skill_copies`]), given `offered`, where what it installed is offered
/// now.
pub(crate) fn skill_standing(
    workspace: &FlayerWorkspace,
    looked: &[Looked],
    offered: &[&Offer],
) -> Standing {
    standing(looked, !offered.is_empty(), |copy| {
        offered.iter().any(|offer| holds(workspace, offer, copy))
    })
}

/// Each copy of `record`, a rule's in `project`, as it is found, by the hash
/// recorded of each: its section in the block of `block`, the project's
/// `AGENTS.md` as it reads now, and a file for Claude or, in a project a
/// `mind.toml` marks, its copy. A target recorded with no hash is held to
/// what `offered` would put there now.
///
/// A section that cannot be read back, and anything at a file's place that
/// is not a file, are edited: something is there, and it is not what was
/// written.
///
/// In a project marked by its settings, a copy from before it had them, in
/// the folder its `mind.toml` named, is not looked at. The next render takes
/// it out while it holds what was copied and leaves it, the project's own,
/// when it does not ([`super::rules`]'s `Copied::take_out`), forced or not:
/// an edit there is nothing a change refuses or writes over, so telling it
/// as edited would refuse a change that `--force` would not make either.
pub(crate) fn rule_copies(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    record: &InstallRecord,
    block: Option<&AgentsMd>,
    offered: &[&Offer],
) -> Vec<Looked> {
    let name = &record.name;
    let rendered = project.settings().is_some();
    let file = super::rules::claude_file(name);
    record
        .targets
        .iter()
        .filter(|target| !rendered || target.path == agents_md::FILE || target.path == file)
        .map(|target| {
            if target.path == agents_md::FILE {
                let path = project.root().join(agents_md::FILE);
                let held = block.filter(|block| block.routes().contains(&name.as_str()));
                let found = match held.map(|block| block.section(name)) {
                    None => Found::Missing,
                    Some(None) => Found::Edited,
                    Some(Some(section)) => {
                        let hash = section.hash();
                        let untouched = match &target.content_hash {
                            Some(written) => hash == *written,
                            None => offered.iter().any(|offer| {
                                section_of(workspace, offer).is_some_and(|offered| offered == hash)
                            }),
                        };
                        if untouched {
                            Found::Untouched
                        } else {
                            Found::Edited
                        }
                    }
                };
                return Looked { path, found };
            }
            let Ok(path) = recorded_copy(project, Kind::Rule, name, &target.path) else {
                return missing(project, target);
            };
            let found = match fs::symlink_metadata(&path) {
                Err(_) => Found::Missing,
                Ok(metadata) if metadata.is_file() => match fs::read(&path) {
                    Ok(bytes) => {
                        let untouched = match &target.content_hash {
                            Some(written) => hash::bytes(&bytes) == *written,
                            None => offered.iter().any(|offer| {
                                fs::read(offer.source(workspace))
                                    .is_ok_and(|offered| offered == bytes)
                            }),
                        };
                        if untouched {
                            Found::Untouched
                        } else {
                            Found::Edited
                        }
                    }
                    Err(_) => Found::Edited,
                },
                Ok(_) => Found::Edited,
            };
            Looked { path, found }
        })
        .collect()
}

/// How the installation `record`, a rule's in `project`, stands, given the
/// block of its `AGENTS.md` (`block`) and `offered`, where what it installed
/// is offered now.
///
/// Outdated once the rule as offered is not the rule as rendered, which the
/// installation's hash names: every copy is rendered from those bytes, the
/// section without its front matter and the file whole.
pub(crate) fn of_rule(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    record: &InstallRecord,
    block: Option<&AgentsMd>,
    offered: &[&Offer],
) -> Standing {
    let looked = rule_copies(workspace, project, record, block, offered);
    rule_standing(workspace, record, &looked, offered)
}

/// How the installation `record`, a rule's, stands from how its copies were
/// `looked` at ([`rule_copies`]), given `offered`, where what it installed
/// is offered now: [`of_rule`], for a caller that keeps the copies too.
pub(crate) fn rule_standing(
    workspace: &FlayerWorkspace,
    record: &InstallRecord,
    looked: &[Looked],
    offered: &[&Offer],
) -> Standing {
    standing(looked, !offered.is_empty(), |_| {
        offered.iter().any(|offer| {
            let bytes = fs::read(offer.source(workspace)).ok();
            match &record.content_hash {
                Some(written) => bytes.is_some_and(|bytes| hash::bytes(&bytes) == *written),
                // Held to what its copies hold, each untouched one having
                // been found to match an offer: any of them.
                None => bytes.is_some(),
            }
        })
    })
}

/// How an installation stands from how its copies are `looked` at, whether
/// the place it came from offers it still (`offered`), and whether what it
/// offers is what an untouched copy holds (`current`, asked of one).
fn standing(looked: &[Looked], offered: bool, current: impl FnOnce(&Path) -> bool) -> Standing {
    if looked.iter().any(|copy| copy.found == Found::Edited) {
        return Standing::Edited;
    }
    if !offered {
        return Standing::Orphaned;
    }
    if looked.iter().any(|copy| copy.found == Found::Missing) {
        return Standing::Missing;
    }
    match looked.first() {
        Some(copy) if !current(&copy.path) => Standing::Outdated,
        // Every copy holds what was installed, so any of them says what
        // that was; one recording no copy has nothing to fall behind.
        _ => Standing::Current,
    }
}

/// A copy recorded at a route no copy is followed to: looked for nowhere,
/// and so missing.
fn missing(project: &MindProject, target: &Target) -> Looked {
    Looked {
        path: project.root().join(&target.path),
        found: Found::Missing,
    }
}

/// Whether the skill at `copy` holds what an install of `offer` would write
/// there: its names and bytes, but for what an install leaves out.
fn holds(workspace: &FlayerWorkspace, offer: &Offer, copy: &Path) -> bool {
    copy::same_leaving(&offer.source(workspace), copy, &left_out_names()).unwrap_or(false)
}

/// The hash of the section `offer`, a rule, renders as: none for one about
/// some files only, which has no section, or one that cannot be read.
fn section_of(workspace: &FlayerWorkspace, offer: &Offer) -> Option<String> {
    let rule = Artifact::rule(
        offer.source(workspace),
        offer.name().to_owned(),
        workspace.root(),
    )
    .ok()?;
    Section::of(&rule)
        .ok()
        .flatten()
        .map(|section| section.hash())
}

#[cfg(test)]
#[path = "standing_test.rs"]
mod tests;
