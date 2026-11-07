//! The skill directories at the command line (#108): `gather search`, asking
//! finders this process serves on 127.0.0.1, a port each, with the fixtures
//! core's suite reads ([`Finders`]), named with `--finder-url`, so nothing
//! reaches the network and no test names GitHub, skills.sh or
//! api.claude-plugins.dev. No real token is used: the one a test sends is
//! a word made up here.

mod common;

use common::finders::{
    finder_fixture, list_finders, CLAUDE_PLUGINS_PDF, GITHUB_PDF, SKILLS_SH_PDF,
};
use common::*;

/// A token made up for the tests, which no service would take.
const TOKEN: &str = "made-up-token-0123456789";

/// A workspace of its own, made, whose marker lists `finders` as somebody
/// would add them by hand.
fn workspace(finders: &[&str]) -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    list_finders(dir.path(), finders);
    dir
}

/// `mindweaver gather search <args>` in `dir`, asking `finders`.
fn search(dir: &Path, finders: &Finders, args: &[&str]) -> Result<Outcome, Failure> {
    let options = finders.options();
    let mut line = vec!["gather", "search"];
    line.extend_from_slice(args);
    line.extend(options.iter().map(String::as_str));
    weaver(dir, &line)
}

/// What every `--json` of a search prints as its data, read back field by
/// field: a name renamed or added fails here before a script reading it.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Found {
    hits: Vec<Hit>,
    skipped: Vec<String>,
    failures: Vec<Said>,
    warnings: Vec<Said>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Hit {
    name: String,
    repository: String,
    path: Option<String>,
    reference: Option<String>,
    installs: Option<u64>,
    stars: Option<u64>,
    finders: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Said {
    code: String,
    message: String,
}

/// Every result, a numbered row each, by its name, its repository, its
/// folder, its installs or stars and the finders that found it, one two
/// finders name listed once; what does not read, or names no git source,
/// a warning with its code, which fails the search as a failure collected
/// does. A finder on this machine may name a folder here, as this one
/// does, which a finder elsewhere may not.
#[test]
fn a_search_lists_each_result_by_its_repository_folder_and_finders() {
    let dir = workspace(&["github", "skills.sh", "claude-plugins.dev"]);
    let finders = Finders::start();

    let outcome = spawned_search(
        dir.path(),
        &finders,
        &["pdf", "--token-env", "MW_SEARCH_TOKEN"],
    );

    assert_eq!(
        outcome.stdout,
        "1  pdf           https://github.com/acme/skills       skills/pdf     12 installs    github, claude-plugins.dev\n\
         2  pdf-skill     https://github.com/acme/pdf-skill    .              1234 installs  github, skills.sh\n\
         3  commit-style  https://github.com/beta/house-rules  .              56 installs    skills.sh\n\
         4  docx          /home/someone/skills                 .              7 installs     skills.sh\n\
         5  slides        https://github.com/beta/decks        skills/slides  7 stars        claude-plugins.dev\n"
    );
    assert_eq!(
        outcome.stderr,
        "warning: github: result 3 (`docs/skill.md`): it is the file `skill.md`, not a `SKILL.md`, so it is not listed\n\
         warning: github: result 4 (`vault`): its repository's address carries a credential, so it is not listed\n\
         warning: skills.sh: result 4 (`broken`): it names no repository, so it is not listed\n\
         warning: claude-plugins.dev: result 3 (`escape`): its folder climbs out of the repository, so it is not listed\n"
    );
    assert!(!outcome.ok);
}

/// Its JSON is core's read type in the envelope, each name a contract,
/// what it could not read said beside the rest as the text says it.
#[test]
fn a_search_as_json_is_what_it_found_in_the_envelope() {
    let dir = workspace(&["skills.sh", "claude-plugins.dev"]);
    let finders = Finders::start();

    let outcome = search(dir.path(), &finders, &["pdf", "--json"]).unwrap();

    let (found, warnings): (Found, _) = enveloped(&outcome);
    let names: Vec<&str> = found.hits.iter().map(|hit| hit.name.as_str()).collect();
    assert_eq!(
        names,
        ["pdf-skill", "commit-style", "docx", "pdf", "slides"]
    );
    let pdf = &found.hits[3];
    assert_eq!(pdf.repository, "https://github.com/acme/skills");
    assert_eq!(pdf.path.as_deref(), Some("skills/pdf"));
    assert_eq!(pdf.reference.as_deref(), Some("main"));
    assert_eq!((pdf.installs, pdf.stars), (Some(12), Some(321)));
    assert_eq!(pdf.finders, ["claude-plugins.dev"]);
    assert!(found.skipped.is_empty());
    let codes: Vec<&str> = found
        .failures
        .iter()
        .map(|said| said.code.as_str())
        .collect();
    assert_eq!(codes, ["finder_result_dropped", "finder_result_dropped"]);
    assert!(found.warnings.is_empty());
    let messages: Vec<String> = found
        .failures
        .iter()
        .map(|said| said.message.clone())
        .collect();
    assert_eq!(warnings, messages);
    assert!(!outcome.ok);
    assert!(finders.github.asked().is_empty());
}

/// A finder the marker does not list is sent nothing, whatever it would
/// answer; one named that it does not list is refused before any is asked;
/// and with none listed, nothing is searched.
#[test]
fn a_finder_not_listed_is_never_asked() {
    let dir = workspace(&["skills.sh"]);
    let finders = Finders::start();

    search(dir.path(), &finders, &["pdf"]).unwrap();
    let refused = search(dir.path(), &finders, &["pdf", "--finder", "github"]).unwrap_err();

    assert_eq!(finders.skills_sh.asked(), [SKILLS_SH_PDF]);
    assert!(finders.github.asked().is_empty());
    assert!(finders.claude_plugins.asked().is_empty());
    assert_eq!(refused.error.code(), "finder_not_enabled");

    let none = workspace(&[]);
    let refused = search(none.path(), &finders, &["pdf"]).unwrap_err();
    assert_eq!(refused.error.code(), "finder_none_enabled");
    assert_eq!(finders.skills_sh.asked(), [SKILLS_SH_PDF]);
}

/// `--finder` asks the finders it names alone, each listed.
#[test]
fn the_finders_named_are_asked_alone() {
    let dir = workspace(&["skills.sh", "claude-plugins.dev"]);
    let finders = Finders::start();

    let outcome = search(
        dir.path(),
        &finders,
        &["pdf", "--finder", "claude-plugins.dev"],
    )
    .unwrap();

    assert!(finders.skills_sh.asked().is_empty());
    assert_eq!(finders.claude_plugins.asked(), [CLAUDE_PLUGINS_PDF]);
    assert!(outcome.stdout.starts_with("1  pdf "), "{}", outcome.stdout);
}

/// A finder whose answer changed shape is a warning with its code, and
/// what the others found is listed all the same.
#[test]
fn a_finder_whose_answer_changed_shape_is_a_warning_beside_the_rest() {
    let dir = workspace(&["skills.sh", "claude-plugins.dev"]);
    let finders = Finders {
        skills_sh: FixtureFinder::answering(
            SKILLS_SH_PDF,
            &finder_fixture("skills-sh-changed.json"),
        ),
        ..Finders::start()
    };

    let outcome = search(dir.path(), &finders, &["pdf"]).unwrap();

    assert!(outcome.stdout.contains("  slides  "), "{}", outcome.stdout);
    assert_eq!(
        outcome.stderr[0].text,
        "warning: skills.sh: its answer is not in the shape this build reads, so nothing it \
         found is listed: missing field `skills`"
    );
    assert_eq!(
        stderr_codes(&outcome)[0].as_deref(),
        Some("finder_answer_unreadable")
    );
    assert!(!outcome.ok);
}

/// A rate limit is a warning saying so and when it resets, with the
/// others' results listed.
#[test]
fn a_rate_limit_is_said_with_when_it_resets() {
    let dir = workspace(&["skills.sh", "claude-plugins.dev"]);
    let answered = (
        429,
        vec![(String::from("Retry-After"), String::from("120"))],
        String::from(r#"{"error": "slow down"}"#),
    );
    let finders = Finders {
        skills_sh: FixtureFinder::serving([(SKILLS_SH_PDF.to_owned(), answered)].into()),
        ..Finders::start()
    };

    let outcome = search(dir.path(), &finders, &["pdf"]).unwrap();

    assert_eq!(
        outcome.stderr[0].text,
        format!(
            "warning: skills.sh: {}{SKILLS_SH_PDF}: rate limited (HTTP 429): it resets in 120 \
             seconds",
            finders.skills_sh.url
        )
    );
    assert_eq!(
        stderr_codes(&outcome)[0].as_deref(),
        Some("http_rate_limited")
    );
    assert!(outcome.stdout.starts_with("1  pdf "), "{}", outcome.stdout);
}

/// GitHub's code search, asked with no token, is skipped and sent nothing,
/// with a hint that fails nothing; one `--token-env` names that is not set
/// is a warning with its code, and GitHub is not asked either.
#[test]
fn github_without_a_token_is_skipped_with_a_hint() {
    let dir = workspace(&["github", "claude-plugins.dev"]);
    let finders = Finders {
        claude_plugins: FixtureFinder::answering(
            CLAUDE_PLUGINS_PDF,
            r#"{"skills": [{"name": "pdf", "sourceUrl": "https://github.com/acme/skills/tree/main/skills/pdf", "stars": 3}]}"#,
        ),
        ..Finders::start()
    };

    let outcome = search(dir.path(), &finders, &["pdf"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "1  pdf  https://github.com/acme/skills#main  skills/pdf  3 stars  claude-plugins.dev\n"
    );
    assert_eq!(
        outcome.stderr,
        ["hint: github was not asked, since its search needs a token: --token-env names the variable that holds one"]
    );
    assert!(outcome.ok);

    let unset = "MINDWEAVER_TEST_TOKEN_NOBODY_SETS";
    let outcome = search(dir.path(), &finders, &["pdf", "--token-env", unset]).unwrap();

    assert_eq!(
        outcome.stderr,
        [format!(
            "warning: ${unset}, which --token-env names, is not set, so no finder that needs a \
             token was asked"
        )]
    );
    assert_eq!(
        stderr_codes(&outcome),
        [Some(String::from("token_env_unset"))]
    );
    assert!(!outcome.ok);
    assert!(finders.github.asked().is_empty());
}

/// A search with nothing to look for asks nothing.
#[test]
fn an_empty_query_is_refused() {
    let dir = workspace(&["skills.sh"]);
    let finders = Finders::start();

    let refused = search(dir.path(), &finders, &[""]).unwrap_err();

    assert_eq!(refused.error.code(), "search_query_empty");
    assert!(finders.skills_sh.asked().is_empty());
}

/// `--finder` and `--finder-url` take a finder by its name alone: the
/// first refused by the parser, the second by core, with its code, and
/// nothing asked; and an address with a credential in it is not repeated.
#[test]
fn a_finder_is_named_by_one_of_core_s_names() {
    let dir = workspace(&["skills.sh"]);
    let finders = Finders::start();
    let line = common::line(
        "mindweaver",
        &["gather", "search", "pdf", "--finder", "skillsmp"],
        dir.path(),
    );
    assert!(WeaverCli::try_parse_from(&line).is_err());

    for at in ["skillsmp=https://x.example", "https://me:hunter2@x.example"] {
        let refused =
            weaver(dir.path(), &["gather", "search", "pdf", "--finder-url", at]).unwrap_err();

        assert_eq!(refused.error.code(), "finder_url_invalid", "{at}");
        assert!(
            !refused.error.to_string().contains("hunter2"),
            "{}",
            refused.error
        );
    }
    assert!(finders.skills_sh.asked().is_empty());
}

// ---------------------------------------------------------------------------
// The token
// ---------------------------------------------------------------------------

/// `mindweaver gather search` in `dir` as the built binary, asking
/// `finders`, with `MW_SEARCH_TOKEN` holding the made-up token: the one
/// way to hand a test's command a variable without setting it for every
/// test in this process.
fn spawned_search(dir: &Path, finders: &Finders, args: &[&str]) -> Spawned {
    let options = finders.options();
    let mut line = vec!["gather", "search"];
    line.extend_from_slice(args);
    line.extend(options.iter().map(String::as_str));
    spawn("mindweaver", dir, &line, &[("MW_SEARCH_TOKEN", TOKEN)])
}

/// The token goes to GitHub's address, as a credential, and to no other
/// finder; it is never printed, and no file the workspace holds, the
/// answers its cache kept among them, has it.
#[test]
fn the_token_goes_to_github_alone_and_is_never_printed_or_kept() {
    let dir = workspace(&["github", "skills.sh", "claude-plugins.dev"]);
    let finders = Finders::start();

    let outcome = spawned_search(
        dir.path(),
        &finders,
        &["pdf", "--token-env", "MW_SEARCH_TOKEN"],
    );

    assert!(
        outcome.stdout.contains("github, claude-plugins.dev"),
        "{}",
        outcome.stdout
    );
    assert_eq!(finders.github.asked(), [GITHUB_PDF]);
    let github = finders.github.heads();
    assert!(
        github[0].to_ascii_lowercase().contains(&format!(
            "authorization: bearer {}",
            TOKEN.to_ascii_lowercase()
        )),
        "{}",
        github[0]
    );
    for head in finders
        .skills_sh
        .heads()
        .iter()
        .chain(&finders.claude_plugins.heads())
    {
        assert!(
            !head.to_ascii_lowercase().contains("authorization"),
            "{head}"
        );
        assert!(!head.contains(TOKEN), "{head}");
    }
    assert!(!outcome.stdout.contains(TOKEN) && !outcome.stderr.contains(TOKEN));
    assert_eq!(files_holding(dir.path(), TOKEN), Vec::<String>::new());
    let kept = dir.path().join(WEAVER_DIR).join("cache").join("http");
    assert_eq!(
        fs::read_dir(kept).unwrap().count(),
        2,
        "the answers asked without it"
    );
}

/// Every file under `dir` whose bytes hold `needle`.
fn files_holding(dir: &Path, needle: &str) -> Vec<String> {
    let mut holding = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if String::from_utf8_lossy(&fs::read(&path).unwrap()).contains(needle) {
                holding.push(path.display().to_string());
            }
        }
    }
    holding
}

// ---------------------------------------------------------------------------
// Taking a result
// ---------------------------------------------------------------------------

/// `--take N` gathers the result listed N, from its repository and its
/// folder, as `gather git --path` gathers it, saying first the line it
/// amounts to: the one skill in that folder, and not its neighbour.
#[test]
fn take_gathers_the_result_listed_from_its_folder() {
    let dir = workspace(&["claude-plugins.dev"]);
    let repository = repository(&[
        (
            "skills/pdf/SKILL.md",
            &skill_file("pdf", "Read and write PDF files"),
        ),
        (
            "skills/docx/SKILL.md",
            &skill_file("docx", "Read and write Word files"),
        ),
    ]);
    let finders = Finders::naming(repository.path());

    let listed = search(dir.path(), &finders, &["pdf"]).unwrap();
    let outcome = search(dir.path(), &finders, &["pdf", "--take", "1"]).unwrap();

    let url = repository.path().display();
    assert!(
        listed.stdout.starts_with(&format!(
            "1  pdf   {url}  skills/pdf   4 stars  claude-plugins.dev\n"
        )),
        "{}",
        listed.stdout
    );
    let mut lines = outcome.stdout.lines();
    assert_eq!(
        lines.next(),
        Some(
            format!("taking result 1, pdf: mindweaver gather git --path skills/pdf {url}").as_str()
        )
    );
    assert!(
        outcome
            .stdout
            .contains("  added  pdf  Read and write PDF files"),
        "{}",
        outcome.stdout
    );
    assert!(!outcome.stdout.contains("docx"), "{}", outcome.stdout);
    assert!(outcome.ok, "{:?}", outcome.stderr);
    let shelf = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert!(shelf.stdout.contains("pdf"), "{}", shelf.stdout);
    assert!(!shelf.stdout.contains("docx"), "{}", shelf.stdout);
}

/// A number the search did not list is refused, and nothing is gathered.
#[test]
fn take_of_a_result_not_listed_gathers_nothing() {
    let dir = workspace(&["claude-plugins.dev"]);
    let repository = repository(&[("skills/pdf/SKILL.md", &skill_file("pdf", "Read PDF files"))]);
    let finders = Finders::naming(repository.path());

    let refused = search(dir.path(), &finders, &["pdf", "--take", "3"]).unwrap_err();

    assert_eq!(refused.error.code(), "search_result_missing");
    assert_eq!(
        refused.error.to_string(),
        "there is no result 3 to take: the search found 2"
    );
    assert!(!dir.path().join(WEAVER_DIR).join("mindweaver.db").exists());
    let line = common::line(
        "mindweaver",
        &["gather", "search", "pdf", "--take", "1", "--json"],
        dir.path(),
    );
    assert!(WeaverCli::try_parse_from(&line).is_err());
}
