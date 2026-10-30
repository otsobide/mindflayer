//! A batch of assignments planned before any of it runs: which offers each
//! project holds, and what a selection of boxes amounts to.
//!
//! Every front end that marks boxes, the install screen and the panel's
//! matrix alike, asks here, so the rules of a batch are said once: which of
//! several offers of one name a project holds, and so which box is ticked.

use super::{is_current, offered_for, project_entry, Candidate, Offer, Standing};
use crate::kind::Kind;
use crate::ledger::{Ledger, LedgerError};
use crate::workspace::{FlayerWorkspace, MindProject};

/// Whether `project` holds each of `candidates`, every offer it is surveyed
/// for ([`super::survey`]): what the box of each shows.
///
/// A name the project holds ticks one of its offers' boxes, since a project
/// has one copy of a name in each folder it installs in, whichever offer it
/// came from: the one its installation came from, as the ledger records it
/// ([`offered_for`]); among several a record from before origins were kept
/// may have come from, or when what it came from offers it no more, the
/// one whose content its copy is ([`is_current`]), and the first when none
/// is. Somebody's ticks every one, being there whichever was asked for and
/// moved by none. A name it does not hold, or whose every copy has gone,
/// ticks none: marking it puts the copies back.
///
/// By the record before the content: an installation outdated or edited
/// since is no offer's content, and the one it came from is still the one
/// an update brings it up to date from, which the panel reads off the
/// snapshot's `origin` too. The content alone, which every installation was
/// told by before origins were kept, ticked the first offer of the name for
/// either.
pub fn holding(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidates: &[Candidate],
) -> Result<Vec<bool>, LedgerError> {
    let entry = project_entry(workspace, project);
    let offers: Vec<Offer> = candidates
        .iter()
        .map(|candidate| candidate.offer.clone())
        .collect();
    let mut held: Vec<bool> = candidates
        .iter()
        .map(|candidate| candidate.standing.present())
        .collect();
    let mut told: Vec<(Kind, &str)> = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let named = (candidate.kind(), candidate.name());
        if !held[index] || candidate.standing == Standing::Foreign || told.contains(&named) {
            continue;
        }
        told.push(named);
        let same: Vec<usize> = (0..candidates.len())
            .filter(|&other| (candidates[other].kind(), candidates[other].name()) == named)
            .collect();
        if same.len() < 2 {
            continue;
        }
        let came: Vec<usize> = match ledger.installation(&entry, named.0, named.1)? {
            Some(record) => offered_for(&record, &offers)
                .into_iter()
                .filter_map(|offer| offers.iter().position(|at| std::ptr::eq(at, offer)))
                .collect(),
            None => Vec::new(),
        };
        let among = if came.is_empty() { &same } else { &came };
        let chosen = match among[..] {
            [one] => one,
            _ => among
                .iter()
                .copied()
                .find(|&at| is_current(workspace, ledger, project, &candidates[at]))
                .unwrap_or(among[0]),
        };
        for at in same {
            held[at] = at == chosen;
        }
    }
    Ok(held)
}
