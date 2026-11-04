//! Mothers of artifacts as their files hold them: a skill's `SKILL.md`, a
//! rule's markdown and an MCP server's definition.

use crate::mother::random;

/// Makes the text of a skill's `SKILL.md`.
pub struct SkillMother;

impl SkillMother {
    /// A skill with a name and a description the Agent Skills format takes,
    /// and the text of its file.
    pub fn random() -> (String, String) {
        let name = random::kebab();
        let text = Self::create(&name, &Self::description());
        (name, text)
    }

    /// `name`'s file, saying it does `description`, front matter first.
    pub fn create(name: &str, description: &str) -> String {
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nSteps.\n")
    }

    /// A description a person writes: a sentence of a few words.
    pub fn description() -> String {
        let words: Vec<String> = (0..random::between(2, 8)).map(|_| random::word()).collect();
        let sentence = words.join(" ");
        let mut characters = sentence.chars();
        let first = characters.next().map(|c| c.to_ascii_uppercase());
        format!(
            "{}{}",
            first.into_iter().collect::<String>(),
            characters.as_str()
        )
    }

    /// `name`'s file with front matter that names no description.
    pub fn without_description(name: &str) -> String {
        format!("---\nname: {name}\n---\n\n# {name}\n")
    }

    /// A file with no front matter at all.
    pub fn without_front_matter(name: &str) -> String {
        format!("# {name}\n\nSteps.\n")
    }

    /// `name`'s file whose description is longer than the format allows.
    pub fn with_long_description(name: &str) -> String {
        Self::create(name, &"a".repeat(crate::skill::MAX_DESCRIPTION_LEN + 1))
    }
}

/// Makes the text of a rule.
pub struct RuleMother;

impl RuleMother {
    /// A rule with something to say and no front matter, as most are.
    pub fn random() -> String {
        Self::create(&random::word())
    }

    /// A rule about `topic`.
    pub fn create(topic: &str) -> String {
        format!("# {topic}\n\nAlways mind the {topic}.\n")
    }

    /// A rule about some files only: front matter naming `paths`.
    pub fn scoped(topic: &str, paths: &[&str]) -> String {
        let listed: String = paths
            .iter()
            .map(|path| format!("  - \"{path}\"\n"))
            .collect();
        format!("---\npaths:\n{listed}---\n\n{}", Self::create(topic))
    }
}

/// Makes the text of an MCP server's definition, a fixture for each
/// transport the format has.
///
/// Each holds what its transport can: inputs of every kind it takes, each
/// written where its kind says, with provenance or extras beside, so a
/// test of one key reads it among the rest. The name and the description
/// are drawn, or given; the rest is fixed, for a test to name what it reads
/// back.
pub struct McpMother;

impl McpMother {
    /// A definition of any of the three transports, under a name an
    /// artifact may be written under, and the text of its file.
    pub fn random() -> (String, String) {
        let name = random::kebab();
        let text = match random::between(0, 2) {
            0 => Self::stdio(&name),
            1 => Self::http(&name),
            _ => Self::sse(&name),
        };
        (name, text)
    }

    /// A program the agent starts: an environment holding a secret and a
    /// setting, an argument somebody supplies, and extras for Claude.
    pub fn stdio(name: &str) -> String {
        format!(
            "{}\n\
             [transport]\n\
             type = \"stdio\"\n\
             command = \"npx\"\n\
             args = [\"-y\", \"@example/server\", \"--workspace\", \"{{workspace}}\"]\n\
             env = {{ API_KEY = \"{{api_key}}\", LOG_LEVEL = \"info\" }}\n\
             cwd = \".\"\n\
             \n\
             [[inputs]]\n\
             name = \"api_key\"\n\
             kind = \"env\"\n\
             description = \"The key the server signs in with\"\n\
             required = true\n\
             secret = true\n\
             env_var = \"EXAMPLE_API_KEY\"\n\
             \n\
             [[inputs]]\n\
             name = \"workspace\"\n\
             kind = \"arg\"\n\
             env_var = \"EXAMPLE_WORKSPACE\"\n\
             \n\
             [extras.claude]\n\
             timeout = 30000\n",
            Self::head(name, &SkillMother::description())
        )
    }

    /// A server at an address with a part of it supplied, reached over
    /// streamable HTTP with a secret header, as a registry offered it.
    pub fn http(name: &str) -> String {
        format!(
            "{}\n\
             [transport]\n\
             type = \"http\"\n\
             url = \"https://{{tenant}}.example.com/mcp\"\n\
             headers = {{ Authorization = \"Bearer {{token}}\" }}\n\
             \n\
             [[inputs]]\n\
             name = \"tenant\"\n\
             kind = \"url_var\"\n\
             required = true\n\
             env_var = \"EXAMPLE_TENANT\"\n\
             \n\
             [[inputs]]\n\
             name = \"token\"\n\
             kind = \"header\"\n\
             required = true\n\
             secret = true\n\
             env_var = \"EXAMPLE_TOKEN\"\n\
             \n\
             [provenance]\n\
             registry = \"https://registry.modelcontextprotocol.io\"\n\
             upstream_name = \"com.example/server\"\n\
             version = \"1.2.0\"\n\
             fetched_at = 2026-11-04T09:30:00Z\n",
            Self::head(name, &SkillMother::description())
        )
    }

    /// A server at an address, reached over server-sent events with a
    /// secret header.
    pub fn sse(name: &str) -> String {
        format!(
            "{}\n\
             [transport]\n\
             type = \"sse\"\n\
             url = \"https://example.com/sse\"\n\
             headers = {{ X-Api-Key = \"{{key}}\" }}\n\
             \n\
             [[inputs]]\n\
             name = \"key\"\n\
             kind = \"header\"\n\
             secret = true\n\
             env_var = \"EXAMPLE_KEY\"\n",
            Self::head(name, &SkillMother::description())
        )
    }

    /// The program of [`stdio`](Self::stdio), saying it is written in
    /// `format`: a newer one than this build reads, or one never written.
    pub fn in_format(name: &str, format: &str) -> String {
        Self::stdio(name).replacen("format = 1\n", &format!("format = {format}\n"), 1)
    }

    /// A program that names nothing to start, and an address with no
    /// address: what `validate` calls incomplete.
    pub fn incomplete(name: &str, transport: &str) -> String {
        format!(
            "{}\n[transport]\ntype = \"{transport}\"\n",
            Self::head(name, &SkillMother::description())
        )
    }

    /// The program of [`stdio`](Self::stdio), with an input nothing uses
    /// and one declared twice.
    pub fn unreferenced(name: &str) -> String {
        format!(
            "{}\n\
             [[inputs]]\n\
             name = \"unused\"\n\
             kind = \"env\"\n\
             \n\
             [[inputs]]\n\
             name = \"workspace\"\n\
             kind = \"arg\"\n",
            Self::stdio(name)
        )
    }

    /// What every definition opens with: its format, its name and what it
    /// is for, each a TOML string whatever it holds.
    pub fn head(name: &str, description: &str) -> String {
        format!(
            "format = 1\nname = {}\ndescription = {}\n",
            toml::Value::String(name.to_owned()),
            toml::Value::String(description.to_owned())
        )
    }
}
