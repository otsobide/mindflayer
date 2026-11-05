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

    /// `name`, saying it is for `description`, reached over `transport`
    /// (`stdio`, `http` or `sse`) as [`stdio`](Self::stdio),
    /// [`http`](Self::http) and [`sse`](Self::sse) reach it: for a fixture
    /// that has to read the same every run.
    pub fn create(name: &str, description: &str, transport: &str) -> String {
        let head = Self::head(name, description);
        match transport {
            "stdio" => Self::program(&head),
            "http" => Self::streamed(&head),
            "sse" => Self::evented(&head),
            other => panic!("{other} is no transport the format has"),
        }
    }

    /// A program the agent starts: an environment holding a secret and a
    /// setting, an argument somebody supplies, and extras for Claude.
    pub fn stdio(name: &str) -> String {
        Self::program(&Self::head(name, &SkillMother::description()))
    }

    /// A server at an address with a part of it supplied, reached over
    /// streamable HTTP with a secret header, as a registry offered it.
    pub fn http(name: &str) -> String {
        Self::streamed(&Self::head(name, &SkillMother::description()))
    }

    /// A server at an address, reached over server-sent events with a
    /// secret header.
    pub fn sse(name: &str) -> String {
        Self::evented(&Self::head(name, &SkillMother::description()))
    }

    fn program(head: &str) -> String {
        format!(
            "{head}\n\
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
        )
    }

    fn streamed(head: &str) -> String {
        format!(
            "{head}\n\
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
        )
    }

    fn evented(head: &str) -> String {
        format!(
            "{head}\n\
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
        )
    }

    /// The address of [`http`](Self::http), its supplied part written in
    /// its userinfo, `https://{tenant}@example.com/mcp`, where an address
    /// is kept with no credential: what `validate` says no agent is given.
    pub fn in_userinfo(name: &str) -> String {
        Self::http(name).replacen(
            "https://{tenant}.example.com/mcp",
            "https://{tenant}@example.com/mcp",
            1,
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

    /// The program of [`stdio`](Self::stdio), its secret input named with
    /// a brace in it, `api}key`, written where it goes all the same, and
    /// read from a variable with hyphens, which no agent can refer to:
    /// what `validate` says no placeholder and no reference can be made of.
    pub fn misnamed(name: &str) -> String {
        Self::stdio(name)
            .replacen("name = \"api_key\"", "name = \"api}key\"", 1)
            .replacen("\"{api_key}\"", "\"{api}key}\"", 1)
            .replacen("\"EXAMPLE_API_KEY\"", "\"EXAMPLE-API-KEY\"", 1)
    }

    /// The program of [`stdio`](Self::stdio), its secret input naming no
    /// variable to be read from: what `validate` says no agent can be given
    /// but as a value a file holds.
    pub fn secret_without_variable(name: &str) -> String {
        Self::stdio(name).replacen("env_var = \"EXAMPLE_API_KEY\"\n", "", 1)
    }

    /// A token of each shape a definition is refused for holding, beside
    /// what it starts with, as `validate` says it.
    ///
    /// Drawn afresh every run and put together from parts at run time, so
    /// that no file in the repository holds anything shaped like a real
    /// credential, which a secret scanner would stop a push for, and a test
    /// cannot pass on one value it happens to spell.
    pub fn tokens() -> Vec<(&'static str, String)> {
        let upper = random::LOWERCASE.to_ascii_uppercase();
        let alphanumeric = format!("{}{upper}{}", random::LOWERCASE, random::DIGITS);
        let drawn = |length: usize| random::drawn_from(&alphanumeric, length);
        let github = random::pick(&["ghp", "gho", "ghu", "ghs", "ghr"]);
        let github_prefix: &'static str = match github {
            "ghp" => "ghp_",
            "gho" => "gho_",
            "ghu" => "ghu_",
            "ghs" => "ghs_",
            _ => "ghr_",
        };
        vec![
            (github_prefix, format!("{github}_{}", drawn(36))),
            (
                "github_pat_",
                format!("github_pat_{}_{}", drawn(22), drawn(59)),
            ),
            (
                "sk-",
                format!(
                    "sk-{}{}{}",
                    random::pick(&["", "proj-", "ant-api03-"]),
                    random::drawn_from(random::DIGITS, 1),
                    drawn(47)
                ),
            ),
            (
                "Bearer",
                format!(
                    "{} {}.{}",
                    random::pick(&["Bearer", "bearer"]),
                    drawn(24),
                    drawn(16)
                ),
            ),
        ]
    }

    /// One token of the shapes [`tokens`](Self::tokens) draws, beside what
    /// it starts with.
    pub fn token() -> (&'static str, String) {
        random::pick(&Self::tokens())
    }

    /// `name`'s definition holding `token` as it is, written where `place`
    /// says (`args`, `env`, `headers`, `url`, or `userinfo`, the user of
    /// the address), beside where `validate` says it is.
    pub fn with_literal(name: &str, place: &str, token: &str) -> (String, String) {
        let quoted = toml::Value::String(token.to_owned()).to_string();
        match place {
            "args" => (
                Self::stdio(name).replacen(
                    "\"{workspace}\"]",
                    &format!("\"{{workspace}}\", {quoted}]"),
                    1,
                ),
                String::from("transport.args[4]"),
            ),
            "env" => (
                Self::stdio(name).replacen(
                    "LOG_LEVEL = \"info\"",
                    &format!("LOG_LEVEL = \"info\", SERVICE_KEY = {quoted}"),
                    1,
                ),
                String::from("transport.env.SERVICE_KEY"),
            ),
            "headers" => (
                Self::http(name).replacen(
                    "Authorization = \"Bearer {token}\"",
                    &format!("Authorization = \"Bearer {{token}}\", X-Service-Key = {quoted}"),
                    1,
                ),
                String::from("transport.headers.X-Service-Key"),
            ),
            "url" => (
                Self::http(name).replacen(
                    "/mcp\"",
                    &format!("/mcp?key={}\"", &quoted[1..quoted.len() - 1]),
                    1,
                ),
                String::from("transport.url"),
            ),
            "userinfo" => (
                Self::http(name).replacen(
                    "https://{tenant}",
                    &format!("https://{}@{{tenant}}", &quoted[1..quoted.len() - 1]),
                    1,
                ),
                String::from("transport.url"),
            ),
            other => panic!("{other} is no place a definition writes a value"),
        }
    }

    /// Texts that start the way a token does and are none, each beside
    /// why: what `validate` lets a definition hold. Each is judged with
    /// `{token}` an input's placeholder.
    pub fn not_tokens() -> Vec<(&'static str, String)> {
        let alphanumeric = format!("{}{}", random::LOWERCASE, random::DIGITS);
        vec![
            ("a placeholder after Bearer", String::from("Bearer {token}")),
            (
                "a reference in Claude's own syntax",
                String::from("Bearer ${GITHUB_TOKEN}"),
            ),
            (
                "a variable a shell expands",
                String::from("Bearer $GITHUB_TOKEN"),
            ),
            (
                "a scheme with nothing after it",
                String::from("--auth-scheme=Bearer"),
            ),
            ("a short word after a scheme", String::from("Bearer abc")),
            (
                "a prefix inside a word",
                format!("risk-{}", random::drawn_from(&alphanumeric, 30)),
            ),
            (
                "a package named with no digit",
                String::from("sk-mcp-server-for-every-tracker"),
            ),
            (
                "a prefix cut short",
                format!("ghp_{}", random::drawn_from(&alphanumeric, 12)),
            ),
            (
                "a prefix in capitals GitHub never gives",
                format!("GHP_{}", random::drawn_from(&alphanumeric, 36)),
            ),
            ("a path", String::from("/usr/local/share/sk-tools")),
            ("a token made of an input", String::from("ghp_{token}")),
        ]
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
