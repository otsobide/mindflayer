//! The skill directories, asked through a transport kept in memory: nothing
//! reaches the network, and no test names GitHub's API, skills.sh or
//! api.claude-plugins.dev but to hold that each is asked at its own
//! address by default, which the fake answers itself. No real token is
//! used: the one sent is a word made up here.
//!
//! Every answer is a fixture under `tests/fixtures/finders/`, written by
//! hand: GitHub's to its documented code search, the other two to the
//! shape each answers in today, which no document promises, so an answer
//! whose shape changed is one of them (`skills-sh-changed.json`). Each
//! holds keys nothing here reads, so that reading past them is held too.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use mindweaver_core::finder::{self, Failure, Finder, Found, Search, Token};
use mindweaver_core::http::{Answer, Cache, Client, Headers, HttpError, Request};
use mindweaver_core::mother::WorkspaceMother;
use mindweaver_core::{Http, WeaverWorkspace};

// ---------------------------------------------------------------------------
// The fake, and the fixtures it answers with
// ---------------------------------------------------------------------------

/// What the fake answers one URL with: a status, headers and a body.
type Reply = (u16, Vec<(String, String)>, String);

/// A request as the fake received it, credentials and all, for a test to
/// ask what was sent where.
#[derive(Debug, Clone)]
struct Sent {
    url: String,
    headers: Vec<(String, String)>,
}

impl Sent {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(each, _)| each.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A transport kept in memory: each URL answered with what it was given,
/// and a 404 for any other; every request it is sent recorded.
#[derive(Debug, Default)]
struct Fake {
    answers: BTreeMap<String, Reply>,
    sent: Mutex<Vec<Sent>>,
}

impl Fake {
    fn answering(self, url: &str, body: &str) -> Self {
        self.answering_with(url, 200, &[], body)
    }

    fn answering_with(
        mut self,
        url: &str,
        status: u16,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Self {
        let headers = headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        self.answers
            .insert(url.to_owned(), (status, headers, body.to_owned()));
        self
    }

    fn sent(&self) -> Vec<Sent> {
        self.sent.lock().unwrap().clone()
    }

    fn urls(&self) -> Vec<String> {
        self.sent().into_iter().map(|sent| sent.url).collect()
    }
}

impl Http for Fake {
    fn send(&self, request: &Request, _: Duration) -> Result<Answer, HttpError> {
        self.sent.lock().unwrap().push(Sent {
            url: request.url().to_owned(),
            headers: request
                .headers()
                .iter()
                .map(|header| (header.name().to_owned(), header.value().to_owned()))
                .collect(),
        });
        let (status, headers, body) = self
            .answers
            .get(request.url())
            .cloned()
            .unwrap_or_else(|| (404, Vec::new(), String::from("not here")));
        let mut answered = Headers::new();
        for (name, value) in headers {
            answered.push(name, value);
        }
        Ok(Answer {
            status,
            headers: answered,
            body: Box::new(io::Cursor::new(body.into_bytes())),
        })
    }
}

/// What the fixture `name` holds.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/finders")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// What is looked for in every test.
const QUERY: &str = "pdf";

/// Where each finder is asked for `pdf`, at its own address.
const GITHUB: &str = "https://api.github.com/search/code?q=pdf+filename%3ASKILL.md&per_page=20";
const SKILLS_SH: &str = "https://skills.sh/api/search?q=pdf&limit=20";
const CLAUDE_PLUGINS: &str = "https://api.claude-plugins.dev/api/skills/search?q=pdf&limit=20";

/// A token made up for the tests, which no service would take.
const TOKEN: &str = "made-up-token-0123456789";

/// A fake answering each finder at its own address with its fixture.
fn every_finder() -> Fake {
    Fake::default()
        .answering(GITHUB, &fixture("github.json"))
        .answering(SKILLS_SH, &fixture("skills-sh.json"))
        .answering(CLAUDE_PLUGINS, &fixture("claude-plugins.json"))
}

fn token() -> Option<Token> {
    Token::new(TOKEN)
}

/// Each result, as a row a test reads: its name, repository, folder,
/// installs, stars and finders.
fn rows(found: &Found) -> Vec<String> {
    found
        .hits
        .iter()
        .map(|hit| {
            let finders: Vec<&str> = hit.finders.iter().map(|finder| finder.name()).collect();
            format!(
                "{} {} {} {:?} {:?} {}",
                hit.name,
                hit.repository,
                hit.path.as_deref().unwrap_or("."),
                hit.installs,
                hit.stars,
                finders.join(",")
            )
        })
        .collect()
}

/// Each failure, by its code and as it is said.
fn said(found: &Found) -> Vec<(String, String)> {
    found
        .failures
        .iter()
        .map(|failure| (failure.code().to_owned(), failure.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// What each finder answers
// ---------------------------------------------------------------------------

/// Every finder's results, a result two of them name listed once with both
/// and the counts each gave: by its repository as every spelling of it is
/// one (`Acme/Skills` is `acme/skills`) and its folder. What does not read,
/// or names no git source, is dropped and said, the credential an address
/// carried never repeated.
#[test]
fn each_finder_lists_what_it_found_once_by_its_repository_and_folder() {
    let fake = every_finder();
    let client = Client::new(&fake);

    let found = Search::new(&client)
        .with_token(token())
        .run(QUERY, &Finder::ALL)
        .unwrap();

    assert_eq!(
        rows(&found),
        [
            "pdf https://github.com/acme/skills skills/pdf Some(12) Some(321) \
             github,claude-plugins.dev",
            "pdf-skill https://github.com/acme/pdf-skill . Some(1234) None github,skills.sh",
            "commit-style https://github.com/beta/house-rules . Some(56) None skills.sh",
            "slides https://github.com/beta/decks skills/slides None Some(7) claude-plugins.dev",
        ]
    );
    assert_eq!(
        said(&found),
        [
            (
                String::from("finder_result_dropped"),
                String::from(
                    "github: result 3 (`docs/skill.md`): it is the file `skill.md`, not a \
                     `SKILL.md`, so it is not listed"
                )
            ),
            (
                String::from("finder_result_dropped"),
                String::from(
                    "github: result 4 (`vault`): its repository's address carries a \
                     credential, so it is not listed"
                )
            ),
            (
                String::from("finder_result_dropped"),
                String::from(
                    "skills.sh: result 3 (`docx`): it names a folder on this machine, which \
                     only a finder on this machine may, so it is not listed"
                )
            ),
            (
                String::from("finder_result_dropped"),
                String::from(
                    "skills.sh: result 4 (`broken`): it names no repository, so it is not \
                     listed"
                )
            ),
            (
                String::from("finder_result_dropped"),
                String::from(
                    "claude-plugins.dev: result 3 (`escape`): its folder climbs out of the \
                     repository, so it is not listed"
                )
            ),
        ]
    );
    assert!(found.skipped.is_empty());
    assert!(found.warnings.is_empty());
    assert_eq!(fake.urls(), [GITHUB, SKILLS_SH, CLAUDE_PLUGINS]);
    let printed = format!("{found:?}");
    assert!(!printed.contains("hunter2"), "{printed}");
}

/// skills.sh names a skill's repository and not its folder, and lists
/// several skills of one repository: each is listed, told apart by its
/// name, and only one it names twice, however it spells the repository,
/// is listed once.
#[test]
fn the_skills_of_one_repository_a_finder_names_no_folder_for_are_each_listed() {
    let answer = r#"{"skills": [
        {"name": "pdf", "source": "acme/skills", "installs": 100},
        {"name": "docx", "source": "acme/skills", "installs": 50},
        {"name": "xlsx", "source": "https://github.com/acme/skills", "installs": 20},
        {"name": "pdf", "source": "Acme/Skills", "installs": 3}
    ]}"#;
    let fake = Fake::default().answering(SKILLS_SH, answer);
    let client = Client::new(&fake);

    let found = Search::new(&client)
        .run(QUERY, &[Finder::SkillsSh])
        .unwrap();

    assert_eq!(
        rows(&found),
        [
            "pdf https://github.com/acme/skills . Some(100) None skills.sh",
            "docx https://github.com/acme/skills . Some(50) None skills.sh",
            "xlsx https://github.com/acme/skills . Some(20) None skills.sh",
        ]
    );
    assert!(found.failures.is_empty(), "{:?}", found.failures);
}

/// The branch a tree URL names is kept with what a finder found there, for
/// the gather to take it.
#[test]
fn a_tree_url_names_the_branch_the_result_is_on() {
    let fake = every_finder();
    let client = Client::new(&fake);

    let found = Search::new(&client)
        .run(QUERY, &[Finder::ClaudePlugins])
        .unwrap();

    assert_eq!(found.hits[0].reference.as_deref(), Some("main"));
    assert_eq!(found.hits[0].path.as_deref(), Some("skills/pdf"));
    assert_eq!(found.hits[1].reference, None);
}

/// A finder whose answer changed shape is a warning of its own, and what
/// the others found is listed all the same.
#[test]
fn a_finder_whose_answer_changed_shape_is_a_warning_beside_the_rest() {
    let fake = every_finder().answering(SKILLS_SH, &fixture("skills-sh-changed.json"));
    let client = Client::new(&fake);

    let found = Search::new(&client)
        .with_token(token())
        .run(QUERY, &Finder::ALL)
        .unwrap();

    let names: Vec<&str> = found.hits.iter().map(|hit| hit.name.as_str()).collect();
    assert_eq!(names, ["pdf", "pdf-skill", "slides"]);
    let unreadable: Vec<&Failure> = found
        .failures
        .iter()
        .filter(|failure| failure.code() == "finder_answer_unreadable")
        .collect();
    assert_eq!(unreadable.len(), 1);
    assert_eq!(
        unreadable[0].to_string(),
        "skills.sh: its answer is not in the shape this build reads, so nothing it found is \
         listed: missing field `skills`"
    );
}

/// An answer that is not JSON at all is the fetch failing, by the client's
/// code, and the rest are listed.
#[test]
fn a_finder_answering_with_what_is_not_json_is_a_warning_beside_the_rest() {
    let fake = every_finder().answering(CLAUDE_PLUGINS, "<html>maintenance</html>");
    let client = Client::new(&fake);

    let found = Search::new(&client)
        .with_token(token())
        .run(QUERY, &Finder::ALL)
        .unwrap();

    assert_eq!(found.hits.len(), 3);
    let codes: Vec<&str> = found.failures.iter().map(Failure::code).collect();
    assert!(codes.contains(&"http_json_invalid"), "{codes:?}");
}

/// A rate limit is said as one, with when it resets, and the others'
/// results are listed.
#[test]
fn a_rate_limited_finder_says_when_it_resets() {
    let fake = every_finder().answering_with(
        GITHUB,
        403,
        &[
            ("X-RateLimit-Remaining", "0"),
            ("X-RateLimit-Reset", "1794043800"),
        ],
        r#"{"message": "API rate limit exceeded"}"#,
    );
    let client = Client::new(&fake);

    let found = Search::new(&client)
        .with_token(token())
        .run(QUERY, &Finder::ALL)
        .unwrap();

    let limited: Vec<String> = found
        .failures
        .iter()
        .filter(|failure| failure.code() == "http_rate_limited")
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        limited,
        [format!(
            "github: {GITHUB}: rate limited (HTTP 403): it resets at 2026-11-07T09:30:00Z"
        )]
    );
    let names: Vec<&str> = found.hits.iter().map(|hit| hit.name.as_str()).collect();
    assert_eq!(names, ["pdf-skill", "commit-style", "pdf", "slides"]);
}

// ---------------------------------------------------------------------------
// Which finders are asked
// ---------------------------------------------------------------------------

/// A workspace whose marker lists `finders`, as somebody would by hand.
fn listing(finders: &str) -> (tempfile::TempDir, WeaverWorkspace) {
    let (dir, workspace) = WorkspaceMother::empty();
    let marker = workspace.config_path();
    let text = fs::read_to_string(&marker).unwrap().replacen(
        "loaded = []\n",
        &format!("loaded = []\nfinders = {finders}\n"),
        1,
    );
    fs::write(&marker, text).unwrap();
    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    (dir, workspace)
}

/// A finder the marker does not list is sent nothing, whatever it would
/// answer, and one named that it does not list is refused before any is
/// asked.
#[test]
fn a_finder_not_enabled_is_never_sent_a_request() {
    let (_dir, workspace) = listing(r#"["skills.sh"]"#);
    let fake = every_finder();
    let client = Client::new(&fake);
    let enabled = finder::enabled(&workspace);

    let asked = enabled.asked(&[]).unwrap();
    let found = Search::new(&client)
        .with_token(token())
        .run(QUERY, &asked)
        .unwrap();

    assert_eq!(fake.urls(), [SKILLS_SH]);
    assert!(found
        .hits
        .iter()
        .all(|hit| hit.finders == [Finder::SkillsSh]));
    assert_eq!(
        enabled.asked(&[Finder::Github]).unwrap_err().code(),
        "finder_not_enabled"
    );
    assert_eq!(fake.urls(), [SKILLS_SH]);
}

/// GitHub's code search with no token is skipped, and sent nothing, rather
/// than asked to answer the 401 it would.
#[test]
fn github_without_a_token_is_skipped_and_sent_nothing() {
    let fake = every_finder();
    let client = Client::new(&fake);

    let found = Search::new(&client).run(QUERY, &Finder::ALL).unwrap();

    assert_eq!(found.skipped, [Finder::Github]);
    assert_eq!(fake.urls(), [SKILLS_SH, CLAUDE_PLUGINS]);
    assert!(found.failures.iter().all(|failure| !matches!(
        failure,
        Failure::Failed {
            finder: Finder::Github,
            ..
        }
    )));
}

#[test]
fn an_empty_query_asks_nothing() {
    let fake = every_finder();
    let client = Client::new(&fake);

    let refused = Search::new(&client).run(" ", &Finder::ALL).unwrap_err();

    assert_eq!(refused.code(), "search_query_empty");
    assert!(fake.urls().is_empty());
}

// ---------------------------------------------------------------------------
// The token
// ---------------------------------------------------------------------------

/// The token goes to GitHub's origin as a credential, and to no other
/// finder; nothing it unlocked is kept in the cache, which keeps the other
/// two's answers, and no file of the workspace holds it.
#[test]
fn the_token_goes_to_github_alone_and_is_never_kept() {
    let (dir, workspace) = WorkspaceMother::empty();
    let fake = every_finder();
    let cache = Cache::of(&workspace).unwrap();
    let client = Client::new(&fake).with_cache(cache);

    let found = Search::new(&client)
        .with_token(token())
        .run(QUERY, &Finder::ALL)
        .unwrap();

    assert_eq!(found.hits.len(), 4);
    for sent in fake.sent() {
        let authorization = sent.header("authorization");
        if sent.url == GITHUB {
            assert_eq!(authorization, Some(format!("Bearer {TOKEN}").as_str()));
            assert_eq!(sent.header("x-github-api-version"), Some("2022-11-28"));
        } else {
            assert_eq!(authorization, None, "{}", sent.url);
        }
    }
    let kept: Vec<String> = fs::read_dir(workspace.http_cache_dir())
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    assert_eq!(kept.len(), 2, "the two answers asked without the token");
    assert!(kept.iter().all(|entry| !entry.contains("acme/vault")));
    assert_eq!(files_holding(dir.path(), TOKEN), Vec::<String>::new());
    // Debugged through a transport that has recorded nothing, which the
    // fake above has, the token among it.
    let quiet = Fake::default();
    let quiet = Client::new(&quiet);
    let debugged = format!("{:?}", Search::new(&quiet).with_token(token()));
    assert!(debugged.contains("Token(<credential>)"), "{debugged}");
    assert!(!debugged.contains(TOKEN), "{debugged}");
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

/// A token is a credential, so a redirect to another origin leaves it
/// behind, as the client leaves every credential.
#[test]
fn the_token_stays_behind_when_github_redirects_elsewhere() {
    let elsewhere = "https://mirror.example/search?q=pdf";
    let fake = Fake::default()
        .answering_with(GITHUB, 302, &[("Location", elsewhere)], "")
        .answering(elsewhere, r#"{"items": []}"#);
    let client = Client::new(&fake);

    let found = Search::new(&client)
        .with_token(token())
        .run(QUERY, &[Finder::Github])
        .unwrap();

    assert!(found.hits.is_empty() && found.failures.is_empty());
    let sent = fake.sent();
    assert_eq!(sent[1].url, elsewhere);
    assert_eq!(sent[1].header("authorization"), None);
}

// ---------------------------------------------------------------------------
// Where a finder is
// ---------------------------------------------------------------------------

/// A finder can be asked at another address speaking its API, below any
/// path the address has, as a GitHub Enterprise server's is.
#[test]
fn a_finder_is_asked_at_the_address_its_caller_names() {
    let fake = Fake::default().answering(
        "https://ghe.example/api/v3/search/code?q=pdf+filename%3ASKILL.md&per_page=20",
        r#"{"items": []}"#,
    );
    let client = Client::new(&fake);

    let search = Search::new(&client)
        .at(Finder::Github, "https://ghe.example/api/v3/")
        .with_token(token());
    let found = search.run(QUERY, &[Finder::Github]).unwrap();

    assert!(found.failures.is_empty(), "{:?}", found.failures);
    assert_eq!(search.base(Finder::Github), "https://ghe.example/api/v3/");
    assert_eq!(search.base(Finder::SkillsSh), "https://skills.sh");
}

/// A finder on this machine, one of a person's own or a test's, may name
/// a repository on this machine, which one elsewhere may not.
#[test]
fn a_finder_on_this_machine_may_name_a_folder_here() {
    let here = "http://127.0.0.1:9";
    let answer = r#"{"skills": [{"name": "docx", "source": "/srv/skills", "installs": 3}]}"#;
    let fake = Fake::default().answering("http://127.0.0.1:9/api/search?q=pdf&limit=20", answer);
    let client = Client::new(&fake);

    let found = Search::new(&client)
        .at(Finder::SkillsSh, here)
        .run(QUERY, &[Finder::SkillsSh])
        .unwrap();

    assert_eq!(rows(&found), ["docx /srv/skills . Some(3) None skills.sh"]);
}

// ---------------------------------------------------------------------------
// A search read again
// ---------------------------------------------------------------------------

/// What a search found is read again from the answers the cache kept of
/// it, sending nothing: what a front end that ran the search as a line
/// reads once the line is over.
#[test]
fn a_search_is_read_again_from_what_the_cache_kept() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let fake = every_finder();
    let client = Client::new(&fake).with_cache(Cache::of(&workspace).unwrap());
    let searched = Search::new(&client).run(QUERY, &Finder::ALL).unwrap();

    let kept = finder::kept(Cache::of(&workspace).unwrap(), QUERY, &Finder::ALL, &[]).unwrap();

    assert_eq!(kept.hits, searched.hits);
    assert_eq!(kept.skipped, [Finder::Github]);
    assert_eq!(said(&kept), said(&searched));
    assert_eq!(fake.urls().len(), 2, "nothing more was sent");
}

/// What GitHub's code search answered a token was never kept, so reading a
/// search again lists what the other finders found, and nothing it alone
/// did.
#[test]
fn what_a_token_unlocked_is_not_read_again() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let fake = every_finder();
    let client = Client::new(&fake).with_cache(Cache::of(&workspace).unwrap());
    Search::new(&client)
        .with_token(token())
        .run(QUERY, &Finder::ALL)
        .unwrap();

    let kept = finder::kept(Cache::of(&workspace).unwrap(), QUERY, &Finder::ALL, &[]).unwrap();

    assert_eq!(
        rows(&kept),
        [
            "pdf-skill https://github.com/acme/pdf-skill . Some(1234) None skills.sh",
            "commit-style https://github.com/beta/house-rules . Some(56) None skills.sh",
            "pdf https://github.com/acme/skills skills/pdf Some(12) Some(321) claude-plugins.dev",
            "slides https://github.com/beta/decks skills/slides None Some(7) claude-plugins.dev",
        ]
    );
}

/// A search whose answers the cache does not keep, as one never made, is
/// no listing at all; one a finder's answer is kept of lists what it
/// found, the finder whose answer is not kept left out.
#[test]
fn a_search_the_cache_keeps_nothing_of_is_no_listing() {
    let (_dir, workspace) = WorkspaceMother::empty();

    assert!(finder::kept(Cache::of(&workspace).unwrap(), QUERY, &Finder::ALL, &[]).is_none());

    let fake = every_finder();
    let client = Client::new(&fake).with_cache(Cache::of(&workspace).unwrap());
    Search::new(&client)
        .run(QUERY, &[Finder::SkillsSh])
        .unwrap();
    let kept = finder::kept(Cache::of(&workspace).unwrap(), QUERY, &Finder::ALL, &[]).unwrap();

    assert!(kept
        .hits
        .iter()
        .all(|hit| hit.finders == [Finder::SkillsSh]));
    assert!(kept
        .failures
        .iter()
        .all(|failure| failure.finder() == Finder::SkillsSh));
}

/// A finder asked at another address is read again at that address, whose
/// answers are the ones kept.
#[test]
fn a_search_at_another_address_is_read_again_there() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let here = "http://127.0.0.1:9";
    let fake = Fake::default().answering(
        "http://127.0.0.1:9/api/search?q=pdf&limit=20",
        r#"{"skills": [{"name": "docx", "source": "acme/docs"}]}"#,
    );
    let client = Client::new(&fake).with_cache(Cache::of(&workspace).unwrap());
    Search::new(&client)
        .at(Finder::SkillsSh, here)
        .run(QUERY, &[Finder::SkillsSh])
        .unwrap();
    let cache = || Cache::of(&workspace).unwrap();

    let at = [(Finder::SkillsSh, String::from(here))];
    let kept = finder::kept(cache(), QUERY, &[Finder::SkillsSh], &at).unwrap();

    assert_eq!(
        rows(&kept),
        ["docx https://github.com/acme/docs . None None skills.sh"]
    );
    assert!(finder::kept(cache(), QUERY, &[Finder::SkillsSh], &[]).is_none());
}
