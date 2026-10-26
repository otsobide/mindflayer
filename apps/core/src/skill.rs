//! What a skill declares about itself.
//!
//! Only the front matter lives here. A skill as a thing on disk is an
//! [`Artifact`](crate::artifact::Artifact) like any other kind; this is the
//! half that is specific to skills.

use std::fmt;

use serde::de::{self, IgnoredAny, Visitor};
use serde::{Deserialize, Deserializer};

/// The longest a single segment of a name may be.
///
/// Per segment rather than per name, because a name can be a route: nesting
/// `git/no-force-push` two folders deep should not spend the budget that
/// exists to keep each part readable.
pub const MAX_NAME_SEGMENT_LEN: usize = 64;
/// The longest description an agent will load.
pub const MAX_DESCRIPTION_LEN: usize = 1024;
/// The longest `compatibility` the specification allows.
pub const MAX_COMPATIBILITY_LEN: usize = 500;

/// The front matter of a `SKILL.md`, as written.
///
/// The fields are the ones the Agent Skills specification defines
/// (<https://agentskills.io/specification>), which is what Claude Code, Codex
/// and OpenCode all read. Unknown keys are kept out of the struct rather than
/// rejected: the format grows, every agent adds keys of its own, and a skill
/// carrying a key this version has never heard of is still a skill worth
/// listing.
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
    /// The license, by name or as a reference to a file the skill bundles.
    #[serde(default)]
    pub license: Option<String>,
    /// What the skill needs from where it runs (the product it was made for,
    /// system packages, network access), in prose, when it needs anything.
    #[serde(default, deserialize_with = "lenient_compatibility")]
    pub compatibility: Compatibility,
    /// Whatever else its author, or a tool, wanted to record about it.
    #[serde(default, deserialize_with = "lenient_metadata")]
    pub metadata: Metadata,
}

impl SkillManifest {
    /// Parse front matter that has already been separated from its body.
    pub fn parse(front_matter: &str) -> Result<Self, serde_yaml_ng::Error> {
        serde_yaml_ng::from_str(front_matter)
    }
}

/// `compatibility`, read leniently.
///
/// The specification makes it a string, but a list of the agents a skill was
/// made for is the obvious thing to write instead, and refusing the whole
/// skill over it would hide it from every listing, as with `metadata`. So a
/// value that is not a string is noted for `validate` to say so, and left in
/// the file as written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Compatibility {
    /// Not written, or written with nothing after it.
    #[default]
    Unstated,
    /// Written as a string, as the specification has it.
    Stated(String),
    /// Written as something else (a list, a map, a number), and so ignored.
    NotAString,
}

impl Compatibility {
    /// What it says, when it was written as a string.
    pub fn stated(&self) -> Option<&str> {
        match self {
            Compatibility::Stated(text) => Some(text),
            Compatibility::Unstated | Compatibility::NotAString => None,
        }
    }
}

fn lenient_compatibility<'de, D>(deserializer: D) -> Result<Compatibility, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match Option::<Text>::deserialize(deserializer)? {
        None => Compatibility::Unstated,
        Some(Text(Some(text))) => Compatibility::Stated(text),
        Some(Text(None)) => Compatibility::NotAString,
    })
}

/// A YAML value: its text when it is a string, and nothing otherwise.
///
/// Whatever is not a string is skipped rather than built, because building it
/// can fail where skipping cannot (a map refuses a repeated key), and one
/// stray key inside a value that is about to be ignored anyway is no reason to
/// fail the manifest.
struct Text(Option<String>);

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(TextVisitor).map(Text)
    }
}

struct TextVisitor;

impl<'de> Visitor<'de> for TextVisitor {
    type Value = Option<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("any YAML value")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Self::Value, E> {
        Ok(Some(text.to_owned()))
    }

    fn visit_string<E: de::Error>(self, text: String) -> Result<Self::Value, E> {
        Ok(Some(text))
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_i128<E: de::Error>(self, _: i128) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_u128<E: de::Error>(self, _: u128) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_bytes<E: de::Error>(self, _: &[u8]) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        IgnoredAny.visit_seq(seq).map(|_| None)
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        IgnoredAny.visit_map(map).map(|_| None)
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: de::EnumAccess<'de>,
    {
        IgnoredAny.visit_enum(data).map(|_| None)
    }
}

/// `metadata`, read leniently.
///
/// The specification makes it a map from strings to strings, but skills in
/// the wild nest maps and write booleans in it, as a shipping agent does
/// (agentskills/agentskills#514), and refusing the whole skill over one
/// entry would hide it from every listing. So the string entries are kept, the
/// others are named so `validate` can say so, and nothing is rewritten: what
/// is ignored here stays in the file for whichever agent wrote it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    /// Every entry mapping a string to a string, in the order written.
    pub entries: Vec<(String, String)>,
    /// The key of every other entry, as written.
    pub ignored: Vec<String>,
    /// Whether `metadata` was not a map at all, and so was ignored whole.
    pub not_a_map: bool,
    /// Every key written more than once after its first time, as written.
    /// Only the first is read: which one an agent reads is up to its YAML
    /// parser, and some refuse the file, so neither is a safe guess.
    pub repeated: Vec<String>,
}

fn lenient_metadata<'de, D>(deserializer: D) -> Result<Metadata, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(MetadataVisitor)
}

/// `metadata` read entry by entry, rather than into a map first.
///
/// A map is built with its keys checked, and one key written twice would fail
/// it and with it the whole manifest. Read an entry at a time, a repeated key
/// is one more thing for `validate` to name.
struct MetadataVisitor;

impl MetadataVisitor {
    fn not_a_map() -> Metadata {
        Metadata {
            not_a_map: true,
            ..Metadata::default()
        }
    }
}

impl<'de> Visitor<'de> for MetadataVisitor {
    type Value = Metadata;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("any YAML value")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        use serde_yaml_ng::Value;

        let mut metadata = Metadata::default();
        let mut seen = std::collections::HashSet::new();
        while let Some(key) = map.next_key::<Value>()? {
            let Text(value) = map.next_value()?;
            let written = written(&key);
            if !seen.insert(written.clone()) {
                metadata.repeated.push(written);
                continue;
            }
            match (key, value) {
                (Value::String(key), Some(value)) => metadata.entries.push((key, value)),
                _ => metadata.ignored.push(written),
            }
        }
        Ok(metadata)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Metadata::default())
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Metadata::default())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(Self::not_a_map())
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(Self::not_a_map())
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(Self::not_a_map())
    }

    fn visit_i128<E: de::Error>(self, _: i128) -> Result<Self::Value, E> {
        Ok(Self::not_a_map())
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(Self::not_a_map())
    }

    fn visit_u128<E: de::Error>(self, _: u128) -> Result<Self::Value, E> {
        Ok(Self::not_a_map())
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(Self::not_a_map())
    }

    fn visit_bytes<E: de::Error>(self, _: &[u8]) -> Result<Self::Value, E> {
        Ok(Self::not_a_map())
    }

    fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        IgnoredAny.visit_seq(seq).map(|_| Self::not_a_map())
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: de::EnumAccess<'de>,
    {
        IgnoredAny.visit_enum(data).map(|_| Self::not_a_map())
    }
}

/// A YAML key as somebody would recognise it in the file: a string as itself,
/// anything else as YAML.
fn written(key: &serde_yaml_ng::Value) -> String {
    match key {
        serde_yaml_ng::Value::String(key) => key.clone(),
        other => serde_yaml_ng::to_string(other)
            .map(|text| text.trim_end().to_owned())
            .unwrap_or_default(),
    }
}

/// `allowed-tools` is written both as a YAML list and as one string, and both
/// spellings mean the same thing.
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
        Some(Written::Inline(line)) => Some(split_tools(&line)),
    })
}

/// The tools in one `allowed-tools` string.
///
/// The Agent Skills specification separates them with spaces. Commas are
/// still accepted, because that is how Claude Code first wrote them and how
/// skills written before the specification spell them. Neither separates
/// inside parentheses: a Claude permission pattern such as `Bash(git add *)`
/// is one tool, and a plain split on whitespace would cut it into three.
fn split_tools(line: &str) -> Vec<String> {
    let mut tools = Vec::new();
    let mut tool = String::new();
    let mut depth = 0usize;
    for c in line.chars() {
        let separates = depth == 0 && (c == ',' || c.is_whitespace());
        if separates {
            if !tool.is_empty() {
                tools.push(std::mem::take(&mut tool));
            }
            continue;
        }
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        tool.push(c);
    }
    if !tool.is_empty() {
        tools.push(tool);
    }
    tools
}

#[cfg(test)]
#[path = "skill_test.rs"]
mod tests;
