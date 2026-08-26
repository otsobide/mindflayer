//! What a skill declares about itself.
//!
//! Only the front matter lives here. A skill as a thing on disk is an
//! [`Artifact`](crate::artifact::Artifact) like any other kind; this is the
//! half that is specific to skills.

use serde::Deserialize;

/// The longest a single segment of a name may be.
///
/// Per segment rather than per name, because a name can be a route: nesting
/// `git/no-force-push` two folders deep should not spend the budget that
/// exists to keep each part readable.
pub const MAX_NAME_SEGMENT_LEN: usize = 64;
/// The longest description an agent will load.
pub const MAX_DESCRIPTION_LEN: usize = 1024;

/// The front matter of a `SKILL.md`, as written.
///
/// Unknown keys are kept out of the struct rather than rejected: the format
/// grows, and a skill carrying a key this version has never heard of is still
/// a skill worth listing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkillManifest {
    /// The name the agent invokes the skill by.
    pub name: String,
    /// When to use the skill. This is the only text an agent sees before
    /// deciding to load the rest, which is why an empty one is a hard error.
    pub description: String,
    /// The tools the skill is allowed to use, if it narrows them.
    #[serde(default, rename = "allowed-tools", deserialize_with = "tool_list")]
    pub allowed_tools: Option<Vec<String>>,
    /// An SPDX identifier, when the skill declares one.
    #[serde(default)]
    pub license: Option<String>,
}

impl SkillManifest {
    /// Parse front matter that has already been separated from its body.
    pub fn parse(front_matter: &str) -> Result<Self, serde_yaml_ng::Error> {
        serde_yaml_ng::from_str(front_matter)
    }
}

/// `allowed-tools` is written both as a YAML list and as one comma separated
/// string, and both spellings mean the same thing.
fn tool_list<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Written {
        Inline(String),
        List(Vec<String>),
    }

    let written = Option::<Written>::deserialize(deserializer)?;
    Ok(match written {
        None => None,
        Some(Written::List(tools)) => Some(tools),
        Some(Written::Inline(line)) => Some(
            line.split(',')
                .map(str::trim)
                .filter(|tool| !tool.is_empty())
                .map(str::to_owned)
                .collect(),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_required_keys() {
        let manifest = SkillManifest::parse("name: pdf-forms\ndescription: Fill PDF forms\n")
            .expect("valid front matter");
        assert_eq!(manifest.name, "pdf-forms");
        assert_eq!(manifest.description, "Fill PDF forms");
        assert_eq!(manifest.allowed_tools, None);
    }

    #[test]
    fn reads_allowed_tools_written_either_way() {
        let inline =
            SkillManifest::parse("name: a\ndescription: d\nallowed-tools: Read, Write , Bash\n")
                .unwrap();
        let list = SkillManifest::parse(
            "name: a\ndescription: d\nallowed-tools:\n  - Read\n  - Write\n  - Bash\n",
        )
        .unwrap();
        let expected = Some(vec!["Read".into(), "Write".into(), "Bash".into()]);
        assert_eq!(inline.allowed_tools, expected);
        assert_eq!(list.allowed_tools, expected);
    }

    #[test]
    fn rejects_front_matter_missing_a_required_key() {
        assert!(SkillManifest::parse("name: a\n").is_err());
        assert!(SkillManifest::parse("description: d\n").is_err());
    }
}
