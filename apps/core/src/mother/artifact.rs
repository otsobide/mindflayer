//! Mothers of artifacts as their files hold them: a skill's `SKILL.md` and a
//! rule's markdown.

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
