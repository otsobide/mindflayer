//! skills.sh's search, `/api/search`: undocumented, and read as the
//! `skills` CLI reads it today, so unstable. Each skill comes with the
//! repository it is in, as GitHub's `owner/repo`, and how many times it was
//! installed; not the folder it is in, so it is gathered from the
//! repository's root, which finds it wherever the common layouts keep it,
//! and brings every other skill the repository holds with it.

use serde::Deserialize;

use super::{below, count, entries, request as asking, Entry, Listed, PER_FINDER};
use crate::http::{HttpError, Request};
use crate::text::printable;

/// What a search of `query` asks at `base`.
pub(super) fn request(base: &str, query: &str) -> Result<Request, HttpError> {
    let limit = PER_FINDER.to_string();
    let url = below(base, &["api", "search"], &[("q", query), ("limit", &limit)])?;
    Ok(asking(&url, "application/json"))
}

#[derive(Deserialize)]
struct Answer {
    skills: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Skill {
    name: Option<String>,
    /// `owner/repo/skill`.
    id: Option<String>,
    source: Option<String>,
    #[serde(rename = "topSource")]
    top_source: Option<String>,
    installs: Option<serde_json::Value>,
}

/// Each skill listed, by its name, in the repository its `source` names,
/// or the first two parts of its `id` when it names none.
pub(super) fn read(answer: serde_json::Value) -> Result<Vec<Entry>, String> {
    let answer: Answer =
        serde_json::from_value(answer).map_err(|error| printable(&error.to_string()))?;
    Ok(entries(answer.skills, "name", |skill: Skill| {
        let from_id = skill.id.as_deref().and_then(|id| {
            let mut parts = id.splitn(3, '/');
            match (parts.next(), parts.next()) {
                (Some(owner), Some(repo)) => Some(format!("{owner}/{repo}")),
                _ => None,
            }
        });
        Ok(Listed {
            name: skill.name,
            address: skill.source.or(skill.top_source).or(from_id),
            installs: count(skill.installs.as_ref()),
            ..Listed::default()
        })
    }))
}
