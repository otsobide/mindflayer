//! Gathering a Claude Code plugin marketplace (#103): its catalog recorded
//! as a parent source, and each plugin picked gathered into a source of its
//! own under it, from repositories on the disk.
//!
//! Nothing reaches the network: a marketplace and every repository its
//! catalog names are folders `gix` clones by their path, each GitHub or
//! other address a catalog names routed to one of them by a remote of the
//! test's own, as #98's and #99's suites reach a GitHub URL.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use common::{commit, repository, skill, url, workspace, write_files};
use mindweaver_core::adapter::gix::Gix;
use mindweaver_core::gather::{self, GatherError, Request, Watch};
use mindweaver_core::marketplace::{self, Holding, Pick};
use mindweaver_core::port::{Cloned, GitError, Remote};
use mindweaver_core::value::Revision;
use mindweaver_core::{Kind, Ledger, SourceKind, WeaverWorkspace};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Where the marketplaces are
// ---------------------------------------------------------------------------

/// A remote that fetches each address it knows from the repository on the
/// disk it routes it to, with gix, and refuses the rest, noting every
/// address it is asked for.
#[derive(Debug, Default)]
struct Routes {
    to: BTreeMap<String, String>,
    asked: Mutex<Vec<String>>,
}

impl Routes {
    fn to(mut self, address: &str, repository: &TempDir) -> Self {
        self.to.insert(address.to_owned(), url(repository));
        self
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Remote for Routes {
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        pin: Option<&Revision>,
        into: &Path,
        watch: &Watch,
    ) -> Result<Cloned, GitError> {
        self.asked.lock().unwrap().push(url.to_owned());
        match self.to.get(url) {
            Some(path) => Gix.fetch(path, reference, pin, into, watch),
            None => Err(GitError::Fetch {
                url: url.to_owned(),
                detail: String::from("no repository there"),
            }),
        }
    }

    fn scrub(&self, clone: &Path) -> Result<bool, GitError> {
        Gix.scrub(clone)
    }
}

/// The marketplace's address, which every test fetches it by.
const MARKET: &str = "https://github.com/acme/market";

/// A request of the marketplace at [`MARKET`], through `routes`.
fn request(routes: &Arc<Routes>, pick: Pick) -> marketplace::Request {
    marketplace::Request::new(
        Request::git(MARKET).through(Arc::clone(routes) as Arc<dyn Remote>),
        pick,
    )
}

/// `names` as a pick of them.
fn plugins(names: &[&str]) -> Pick {
    Pick::Plugins(names.iter().map(|name| (*name).to_owned()).collect())
}

/// The commit `repository`'s HEAD is at.
fn head_of(repository: &TempDir) -> String {
    gix::open(repository.path())
        .unwrap()
        .head_id()
        .unwrap()
        .to_string()
}

/// A catalog listing `plugins`, each an entry as JSON, called `name`.
fn catalog(name: &str, plugins: &[String]) -> String {
    format!(
        r#"{{"name": "{name}", "plugins": [{}]}}"#,
        plugins.join(",")
    )
}

/// A marketplace as anthropics/skills keeps one: two plugins, each the
/// whole of its repository and named by the skills it is made of, and the
/// skills themselves under `skills/`.
fn anthropic() -> TempDir {
    repository(&[
        (
            ".claude-plugin/marketplace.json",
            r#"{
  "name": "anthropic-agent-skills",
  "owner": {"name": "Keith Lazuka", "email": "klazuka@anthropic.com"},
  "metadata": {"description": "Anthropic example skills", "version": "1.0.0"},
  "plugins": [
    {
      "name": "document-skills",
      "description": "Collection of document processing suite",
      "source": "./",
      "strict": false,
      "skills": ["./skills/xlsx", "./skills/docx", "./skills/pdf"]
    },
    {
      "name": "example-skills",
      "description": "Collection of example skills",
      "source": "./",
      "strict": false,
      "skills": ["./skills/algorithmic-art", "./skills/brand-guidelines"]
    }
  ]
}"#,
        ),
        ("skills/xlsx/SKILL.md", &skill("xlsx", "Spreadsheets")),
        ("skills/docx/SKILL.md", &skill("docx", "Documents")),
        (
            "skills/pdf/SKILL.md",
            "---\nname: pdf\ndescription: PDFs\nlicense: Proprietary\n---\n",
        ),
        (
            "skills/algorithmic-art/SKILL.md",
            &skill("algorithmic-art", "Art"),
        ),
        (
            "skills/brand-guidelines/SKILL.md",
            &skill("brand-guidelines", "Brands"),
        ),
        ("skills/brand-guidelines/LICENSE.txt", "Apache License\n"),
        ("README.md", "Agent skills\n"),
    ])
}

/// What `ledger` has gathered, each artifact by its name and the name of
/// the source it came from.
fn shelf(ledger: &Ledger) -> Vec<(String, String)> {
    let mut shelf: Vec<(String, String)> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| (gathered.name.to_string(), gathered.source.name.to_string()))
        .collect();
    shelf.sort();
    shelf
}

fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[test]
fn listing_a_marketplace_records_its_catalog_under_its_own_name_and_gathers_no_plugin() {
    let market = anthropic();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report =
        marketplace::gather(&workspace, &ledger, &request(&routes, Pick::Listing)).unwrap();

    assert_eq!(report.catalog.name, "anthropic-agent-skills");
    assert_eq!(report.catalog.url, MARKET);
    assert_eq!(
        report.catalog.revision.as_deref(),
        Some(head_of(&market).as_str())
    );
    assert_eq!(report.catalog.listed, 2);
    let names: Vec<&str> = report
        .catalog
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, ["document-skills", "example-skills"]);
    assert!(report.children.is_empty());
    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1, "{sources:#?}");
    assert_eq!(sources[0].kind, SourceKind::Marketplace);
    assert_eq!(sources[0].name, "anthropic-agent-skills");
    assert_eq!(
        sources[0].revision.as_deref(),
        Some(head_of(&market).as_str())
    );
    assert!(ledger.gathered().unwrap().is_empty());
    assert!(workspace
        .cache_dir()
        .join(&sources[0].directory)
        .join(marketplace::CATALOG)
        .is_file());
    assert_eq!(routes.asked(), [MARKET]);
}

#[test]
fn a_catalog_of_more_than_a_hundred_plugins_is_listed_without_cloning_any_of_them() {
    let entries: Vec<String> = (0..120)
        .map(|n| {
            format!(
                r#"{{"name": "plugin-{n}", "source": {{"source": "github", "repo": "acme/plugin-{n}"}}}}"#
            )
        })
        .collect();
    let market = repository(&[(".claude-plugin/marketplace.json", &catalog("big", &entries))]);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report =
        marketplace::gather(&workspace, &ledger, &request(&routes, Pick::Listing)).unwrap();

    assert_eq!(report.catalog.listed, 120);
    assert_eq!(report.catalog.entries.len(), 120);
    assert!(report.catalog.needs_confirming());
    // The catalog alone, once: no plugin's repository is asked for.
    assert_eq!(routes.asked(), [MARKET]);
    assert_eq!(ledger.sources().unwrap().len(), 1);
}

/// A marketplace, `big`, listing thirty plugins, each a folder of its
/// repository holding one skill: more than `--all` gathers without a yes.
fn large() -> TempDir {
    let entries: Vec<String> = (0..30)
        .map(|n| format!(r#"{{"name": "plugin-{n}", "source": "./plugins/{n}"}}"#))
        .collect();
    let mut files: Vec<(String, String)> = (0..30)
        .map(|n| {
            (
                format!("plugins/{n}/skills/s{n}/SKILL.md"),
                skill(&format!("s{n}"), "One of many"),
            )
        })
        .collect();
    files.push((
        String::from(".claude-plugin/marketplace.json"),
        catalog("big", &entries),
    ));
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    repository(&borrowed)
}

#[test]
fn every_plugin_of_a_large_catalog_is_looked_at_writing_nothing() {
    let market = large();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let home = TempDir::new().unwrap();
    // A workspace nothing has made yet, as a dry run of a gather is handed.
    let workspace = WeaverWorkspace::unmade(home.path()).unwrap();
    let ledger = workspace.ledger_for_reading().unwrap();

    let listed = marketplace::look(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    assert_eq!(listed.listed, 30);
    assert!(listed.needs_confirming());
    assert_eq!(listed.name, "big");
    // No workspace, no cache and no ledger made, let alone a source.
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn every_plugin_of_a_large_catalog_is_gathered_only_when_told_yes() {
    let market = large();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    // Whatever a front end looked at before: the catalog this fetches is
    // the one held to it.
    let error = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap_err();

    assert_eq!(error.code(), "marketplace_all_unconfirmed");
    assert_eq!(
        error.to_string(),
        "the marketplace big lists 30 plugins, more than 25: --all gathers every one of them \
         only with --yes"
    );
    assert!(ledger.gathered().unwrap().is_empty());
    // The catalog recorded, as a listing records it, and no plugin.
    assert_eq!(ledger.sources().unwrap().len(), 1);

    let report =
        marketplace::gather(&workspace, &ledger, &request(&routes, Pick::AllConfirmed)).unwrap();

    assert_eq!(report.children.len(), 30);
    assert_eq!(ledger.gathered().unwrap().len(), 30);
}

#[test]
fn a_catalog_of_twenty_five_plugins_or_fewer_needs_no_yes_to_be_gathered_whole() {
    let entries: Vec<String> = (0..marketplace::UNCONFIRMED)
        .map(|n| format!(r#"{{"name": "p{n}", "source": "./p{n}"}}"#))
        .collect();
    let mut files: Vec<(String, String)> = (0..marketplace::UNCONFIRMED)
        .map(|n| (format!("p{n}/SKILL.md"), skill(&format!("p{n}"), "One")))
        .collect();
    files.push((
        String::from(".claude-plugin/marketplace.json"),
        catalog("small", &entries),
    ));
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let market = repository(&borrowed);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let listed = marketplace::look(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    assert!(!listed.needs_confirming());
}

#[test]
fn what_a_marketplace_listed_is_read_again_from_its_clone_without_fetching() {
    let market = anthropic();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let listing = request(&routes, Pick::Listing);
    let listed = marketplace::gather(&workspace, &ledger, &listing)
        .unwrap()
        .catalog;

    let recorded = marketplace::recorded(&workspace, &ledger, &listing.catalog).unwrap();

    assert_eq!(recorded, Some(listed));
    assert_eq!(routes.asked(), [MARKET]);
    let elsewhere = Request::git("https://github.com/acme/elsewhere");
    assert_eq!(
        marketplace::recorded(&workspace, &ledger, &elsewhere).unwrap(),
        None
    );
}

/// Asked for by its address alone, as the panel asks once its listing ran,
/// what was listed is what [`marketplace::recorded`] reads, and an address
/// nothing was listed at is refused with a code of its own, said without
/// any credential it was typed with.
#[test]
fn a_marketplace_asked_for_by_its_address_is_what_was_listed_or_refused_as_not_recorded() {
    let market = anthropic();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let listing = request(&routes, Pick::Listing);
    let listed = marketplace::gather(&workspace, &ledger, &listing)
        .unwrap()
        .catalog;

    assert_eq!(
        marketplace::listed(&workspace, &ledger, &listing.catalog).unwrap(),
        listed
    );
    let elsewhere = Request::git("https://u:t0ken@github.com/acme/elsewhere");
    let refused = marketplace::listed(&workspace, &ledger, &elsewhere).unwrap_err();
    assert_eq!(refused.code(), "marketplace_not_recorded");
    assert_eq!(
        refused.to_string(),
        "no marketplace is recorded at https://github.com/acme/elsewhere: `gather marketplace \
         --list` lists its plugins first"
    );
}

// ---------------------------------------------------------------------------
// Gathering the plugins picked
// ---------------------------------------------------------------------------

#[test]
fn a_plugin_picked_is_gathered_from_the_marketplaces_clone_into_a_source_of_its_own() {
    let market = anthropic();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(
        &workspace,
        &ledger,
        &request(&routes, plugins(&["document-skills"])),
    )
    .unwrap();

    assert_eq!(report.children.len(), 1);
    let child = &report.children[0];
    assert_eq!(child.entry, "document-skills");
    let gathered = child.outcome.as_ref().unwrap();
    assert_eq!(gathered.name, "anthropic-agent-skills.document-skills");
    assert_eq!(gathered.added.len(), 3, "{gathered:?}");
    // The skills its entry names, and none of the other plugin's.
    assert_eq!(
        shelf(&ledger),
        pairs(&[
            ("docx", "anthropic-agent-skills.document-skills"),
            ("pdf", "anthropic-agent-skills.document-skills"),
            ("xlsx", "anthropic-agent-skills.document-skills"),
        ])
    );
    // From the marketplace's clone, which was the one fetch.
    assert_eq!(routes.asked(), [MARKET]);
    let parent = ledger
        .sources()
        .unwrap()
        .into_iter()
        .find(|source| source.kind == SourceKind::Marketplace)
        .unwrap();
    let children = ledger.children(parent.id).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].entry.as_deref(), Some("document-skills"));
    assert_eq!(children[0].url, MARKET);
    assert_eq!(children[0].revision, parent.revision);
    // Its upstream path is where it is in the marketplace's repository.
    let upstream: Vec<String> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| gathered.upstream_path)
        .collect();
    assert!(
        upstream.contains(&String::from("skills/xlsx")),
        "{upstream:?}"
    );
}

#[test]
fn every_plugin_of_a_marketplace_is_gathered_with_all_each_into_its_own_source() {
    let market = anthropic();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    let entries: Vec<&str> = report
        .children
        .iter()
        .map(|child| child.entry.as_str())
        .collect();
    assert_eq!(entries, ["document-skills", "example-skills"]);
    assert_eq!(shelf(&ledger).len(), 5);
    // Each is a source of its own, though both are the whole of one
    // repository at one branch.
    assert_eq!(ledger.sources().unwrap().len(), 3);
}

#[test]
fn a_plugin_the_catalog_does_not_list_refuses_the_pick_before_any_is_gathered() {
    let market = anthropic();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let error = marketplace::gather(
        &workspace,
        &ledger,
        &request(&routes, plugins(&["document-skills", "nothing-like-it"])),
    )
    .unwrap_err();

    assert_eq!(error.code(), "marketplace_plugin_not_listed");
    assert_eq!(
        error.to_string(),
        "the marketplace anthropic-agent-skills lists no plugin called `nothing-like-it`: `--list` \
         lists them"
    );
    assert!(ledger.gathered().unwrap().is_empty());
    // The catalog was read, and is recorded as a listing would record it.
    assert_eq!(ledger.sources().unwrap().len(), 1);
}

#[test]
fn a_plugins_licence_is_a_skills_that_says_none_of_its_own() {
    let market = repository(&[
        (
            ".claude-plugin/marketplace.json",
            r#"{"name": "m", "plugins": [{"name": "kit", "source": "./kit", "license": "MIT"}]}"#,
        ),
        (
            "kit/skills/silent/SKILL.md",
            &skill("silent", "Says nothing"),
        ),
        (
            "kit/skills/stated/SKILL.md",
            "---\nname: stated\ndescription: Says it\nlicense: Apache-2.0\n---\n",
        ),
    ]);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    let licences: Vec<(String, Option<String>)> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| (gathered.name.to_string(), gathered.license))
        .collect();
    assert_eq!(
        licences,
        [
            (String::from("silent"), Some(String::from("MIT"))),
            (String::from("stated"), Some(String::from("Apache-2.0"))),
        ]
    );
}

/// A marketplace whose catalog mixes a plugin in its own repository, one in
/// a folder of another repository, pinned, one in a repository of its own,
/// pinned to its first commit, and one made by running a command; and the
/// repositories, the last moved on past its pin.
struct Mixed {
    market: TempDir,
    mono: TempDir,
    tools: TempDir,
    /// The commit `tools` is pinned to, before it moved on.
    pinned: String,
    /// The commit `mono` is pinned to, its only one.
    mono_head: String,
}

fn mixed() -> Mixed {
    let tools = repository(&[("skills/lint/SKILL.md", &skill("lint", "Lint it"))]);
    let pinned = head_of(&tools);
    write_files(
        tools.path(),
        &[
            (
                "skills/lint/SKILL.md",
                &skill("lint", "Lint it, after the pin"),
            ),
            (
                "skills/later/SKILL.md",
                &skill("later", "Added after the pin"),
            ),
        ],
    );
    commit(tools.path());
    let mono = repository(&[
        (
            "tools/review/skills/review/SKILL.md",
            &skill("review", "Review it"),
        ),
        (
            "other/skills/other/SKILL.md",
            &skill("other", "Not this plugin's"),
        ),
    ]);
    let mono_head = head_of(&mono);
    let entries = [
        String::from(
            r#"{"name": "ops", "source": "./plugins/ops", "hooks": "./hooks/hooks.json"}"#,
        ),
        format!(
            r#"{{"name": "review", "source": {{"source": "git-subdir", "url": "https://git.example.com/acme/mono.git", "path": "tools/review", "sha": "{mono_head}"}}}}"#
        ),
        format!(
            r#"{{"name": "tools", "source": {{"source": "github", "repo": "acme/tools", "sha": "{pinned}"}}, "license": "MIT"}}"#
        ),
        String::from(
            r#"{"name": "run-me", "source": {"source": "command", "command": "touch /tmp/never"}}"#,
        ),
        String::from(r#"{"name": "out", "source": "../outside"}"#),
    ];
    let market = repository(&[
        (
            ".claude-plugin/marketplace.json",
            &catalog("mixed", &entries),
        ),
        (
            "plugins/ops/skills/deploy/SKILL.md",
            &skill("deploy", "Ship it"),
        ),
        ("plugins/ops/.mcp.json", "{}"),
        ("plugins/ops/agents/helper.md", "an agent"),
    ]);
    Mixed {
        market,
        mono,
        tools,
        pinned,
        mono_head,
    }
}

fn routed(mixed: &Mixed) -> Arc<Routes> {
    Arc::new(
        Routes::default()
            .to(MARKET, &mixed.market)
            .to("https://git.example.com/acme/mono.git", &mixed.mono)
            .to("https://github.com/acme/tools", &mixed.tools),
    )
}

#[test]
fn a_catalog_mixing_kinds_of_source_gathers_each_plugin_picked_from_where_it_is() {
    let mixed = mixed();
    let routes = routed(&mixed);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    let gathered: Vec<(&str, usize)> = report
        .children
        .iter()
        .map(|child| {
            (
                child.entry.as_str(),
                child.outcome.as_ref().unwrap().added.len(),
            )
        })
        .collect();
    assert_eq!(gathered, [("ops", 1), ("review", 1), ("tools", 1)]);
    assert_eq!(
        shelf(&ledger),
        pairs(&[
            ("deploy", "mixed.ops"),
            ("lint", "mixed.tools"),
            ("review", "mixed.review"),
        ])
    );
    // The catalog, then each repository of its own, once.
    assert_eq!(
        routes.asked(),
        [
            MARKET,
            "https://git.example.com/acme/mono.git",
            "https://github.com/acme/tools"
        ]
    );
    // What a plugin holds beside its skills is said, and not taken.
    assert_eq!(
        report.children[0].holds,
        [Holding::Hooks, Holding::Agents, Holding::McpServers]
    );
    let deploy = workspace.gathered_dir(Kind::Skill);
    assert!(!fs::read_dir(&deploy)
        .unwrap()
        .flatten()
        .any(|folder| folder.path().join(".mcp.json").exists()));
}

#[test]
fn a_pinned_plugin_is_gathered_at_its_pin_and_its_source_keeps_it() {
    let mixed = mixed();
    let routes = routed(&mixed);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    marketplace::gather(
        &workspace,
        &ledger,
        &request(&routes, plugins(&["tools", "review"])),
    )
    .unwrap();

    let sources = ledger.sources().unwrap();
    let tools = sources
        .iter()
        .find(|source| source.name == "mixed.tools")
        .unwrap();
    assert_eq!(tools.pin.as_deref(), Some(mixed.pinned.as_str()));
    assert_eq!(tools.revision.as_deref(), Some(mixed.pinned.as_str()));
    assert_eq!(tools.url, "https://github.com/acme/tools");
    let review = sources
        .iter()
        .find(|source| source.name == "mixed.review")
        .unwrap();
    assert_eq!(review.pin.as_deref(), Some(mixed.mono_head.as_str()));
    assert_eq!(review.subdirectory, "tools/review");
    // What the pin holds, and not what came after it.
    let lint = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .find(|gathered| gathered.name == "lint")
        .unwrap();
    assert_eq!(lint.revision.as_deref(), Some(mixed.pinned.as_str()));
    assert_eq!(lint.summary.as_deref(), Some("Lint it"));
    assert_eq!(lint.license.as_deref(), Some("MIT"));
    assert!(shelf(&ledger).iter().all(|(name, _)| name != "later"));
}

#[test]
fn a_command_entry_and_one_leading_out_of_the_repository_are_refused_and_run_nothing() {
    let mixed = mixed();
    let routes = routed(&mixed);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    let failed: Vec<(Option<&str>, &str)> = report
        .catalog
        .failures
        .iter()
        .map(|failure| (failure.name(), failure.code()))
        .collect();
    assert_eq!(
        failed,
        [
            (Some("run-me"), "marketplace_command_refused"),
            (Some("out"), "marketplace_entry_outside"),
        ]
    );
    assert_eq!(
        report.catalog.failures[0].to_string(),
        "`run-me`: its plugin is made by running a command, which is never run, so it is refused"
    );
    assert!(report
        .children
        .iter()
        .all(|child| child.entry != "run-me" && child.entry != "out"));
    assert!(!Path::new("/tmp/never").exists());
}

#[test]
fn a_header_helper_a_catalog_names_is_never_run() {
    let tools = repository(&[("skills/lint/SKILL.md", &skill("lint", "Lint it"))]);
    let dir = TempDir::new().unwrap();
    let witness = dir.path().join("ran");
    let helper = format!("touch {}", witness.display());
    let entry = format!(
        r#"{{"name": "tools", "source": {{"source": "url", "url": "https://git.example.com/acme/tools.git", "headersHelper": "{helper}"}}}}"#
    );
    let market = repository(&[(".claude-plugin/marketplace.json", &catalog("m", &[entry]))]);
    let routes = Arc::new(
        Routes::default()
            .to(MARKET, &market)
            .to("https://git.example.com/acme/tools.git", &tools),
    );
    let (_ws, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    assert!(report.children[0].outcome.is_ok());
    assert!(!witness.exists());
}

#[test]
fn a_plugins_folders_are_its_own_whatever_its_entry_spells() {
    // Each read as a URL is, what follows its `@` or its `://` alone, would
    // be the shelf's folder, the shelf itself, and the cache's folder.
    let entries = [
        r#"{"name": "x@..", "source": "./a"}"#,
        r#"{"name": "x@.", "source": "./b"}"#,
        r#"{"name": "a://..", "source": {"source": "url", "url": "https://git.example.com/acme/tools.git"}}"#,
    ]
    .map(String::from);
    let tools = repository(&[("skills/lint/SKILL.md", &skill("lint", "Lint it"))]);
    let market = repository(&[
        (".claude-plugin/marketplace.json", &catalog("m", &entries)),
        // Named as the shelf's own folder is, which a copy into the shelf's
        // parent would replace.
        ("a/skills/skills/SKILL.md", &skill("skills", "Named so")),
        ("b/skills/probe/SKILL.md", &skill("probe", "Probe")),
    ]);
    let routes = Arc::new(
        Routes::default()
            .to(MARKET, &market)
            .to("https://git.example.com/acme/tools.git", &tools),
    );
    let (_ws, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let kept = workspace.gathered_dir(Kind::Skill).join("kept");
    fs::create_dir_all(&kept).unwrap();
    fs::write(kept.join("notes"), "mine").unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    for child in &report.children {
        assert!(child.outcome.is_ok(), "{child:?}");
    }
    let mut folders: Vec<String> = ledger
        .sources()
        .unwrap()
        .into_iter()
        .filter(|source| source.parent_id.is_some())
        .map(|source| source.directory)
        .collect();
    folders.sort();
    assert_eq!(
        folders,
        [
            "github.com-acme-market-a-..",
            "github.com-acme-market-x-.",
            "github.com-acme-market-x-..",
        ]
    );
    let shelf = workspace.gathered_dir(Kind::Skill);
    assert!(shelf
        .join("github.com-acme-market-x-../skills/SKILL.md")
        .is_file());
    assert!(shelf
        .join("github.com-acme-market-x-./probe/SKILL.md")
        .is_file());
    assert!(workspace
        .cache_dir()
        .join("github.com-acme-market-a-..")
        .is_dir());
    // Nothing outside the source's own folders was written or taken.
    assert_eq!(fs::read_to_string(kept.join("notes")).unwrap(), "mine");
    assert!(workspace.ledger_path().is_file());
    assert!(!workspace.weaver_dir().join("probe").exists());
}

#[test]
fn a_plugin_that_is_one_skill_is_gathered_without_its_hooks_agents_and_servers() {
    // One found by its root's `SKILL.md`, and one whose entry names its
    // root as its skill.
    let entries = [
        r#"{"name": "solo", "source": "./solo"}"#,
        r#"{"name": "listed", "source": "./listed", "skills": ["./"]}"#,
    ]
    .map(String::from);
    let mut files = vec![(
        String::from(".claude-plugin/marketplace.json"),
        catalog("m", &entries),
    )];
    for plugin in ["solo", "listed"] {
        files.extend([
            (format!("{plugin}/SKILL.md"), skill(plugin, "One skill")),
            (format!("{plugin}/notes.md"), String::from("its own")),
            (format!("{plugin}/hooks/hooks.json"), String::from("{}")),
            (
                format!("{plugin}/agents/helper.md"),
                String::from("an agent"),
            ),
            (format!("{plugin}/.mcp.json"), String::from("{}")),
        ]);
    }
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let market = repository(&files);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_ws, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    assert_eq!(report.children.len(), 2);
    for child in &report.children {
        let gathered = child.outcome.as_ref().unwrap();
        assert_eq!(gathered.added.len(), 1, "{child:?}");
        let copy = &gathered.added[0].path;
        assert!(copy.join("SKILL.md").is_file() && copy.join("notes.md").is_file());
        for held in ["hooks", "agents", ".mcp.json"] {
            assert!(!copy.join(held).exists(), "{}: {held}", child.entry);
        }
        // Said, as of a plugin whose skills are folders of it.
        assert_eq!(
            child.holds,
            [Holding::Hooks, Holding::Agents, Holding::McpServers]
        );
    }
    // Compared without them too, so a gather again changes nothing.
    let again = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();
    for child in &again.children {
        let gathered = child.outcome.as_ref().unwrap();
        assert_eq!(gathered.unchanged.len(), 1, "{child:?}");
    }
}

#[test]
fn a_plugins_mcp_file_is_said_by_its_name_and_never_taken() {
    let entries = [
        r#"{"name": "kit", "source": "./kit"}"#,
        r#"{"name": "solo", "source": "./solo"}"#,
        r#"{"name": "both", "source": "./both"}"#,
    ]
    .map(String::from);
    let market = repository(&[
        (".claude-plugin/marketplace.json", &catalog("m", &entries)),
        ("kit/skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("kit/mcp.json", r#"{"mcpServers": {"x": {"command": "x"}}}"#),
        ("solo/SKILL.md", &skill("solo", "One skill")),
        ("solo/mcp.json", "{}"),
        ("both/skills/lint/SKILL.md", &skill("lint", "Lint it")),
        ("both/.mcp.json", "{}"),
        ("both/mcp.json", "{}"),
    ]);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    let said: Vec<(&str, &[Holding], &[&str])> = report
        .children
        .iter()
        .map(|child| {
            (
                child.entry.as_str(),
                child.holds.as_slice(),
                child.mcp_files.as_slice(),
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            ("kit", &[Holding::McpServers][..], &["mcp.json"][..]),
            ("solo", &[Holding::McpServers][..], &["mcp.json"][..]),
            (
                "both",
                &[Holding::McpServers][..],
                &[".mcp.json", "mcp.json"][..]
            ),
        ]
    );
    // A plugin that is one skill is put on the shelf without it, and no
    // plugin's servers are imported: the shelf holds skills alone.
    let solo = &report.children[1].outcome.as_ref().unwrap().added[0].path;
    assert!(solo.join("SKILL.md").is_file() && !solo.join("mcp.json").exists());
    assert!(ledger
        .gathered()
        .unwrap()
        .iter()
        .all(|gathered| gathered.kind == Kind::Skill));
}

#[test]
fn a_plugin_that_cannot_be_fetched_is_said_and_the_rest_gathered() {
    let mixed = mixed();
    // The marketplace and the monorepo, and nothing at acme/tools.
    let routes = Arc::new(
        Routes::default()
            .to(MARKET, &mixed.market)
            .to("https://git.example.com/acme/mono.git", &mixed.mono),
    );
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    let outcomes: Vec<(&str, bool)> = report
        .children
        .iter()
        .map(|child| (child.entry.as_str(), child.outcome.is_ok()))
        .collect();
    assert_eq!(
        outcomes,
        [("ops", true), ("review", true), ("tools", false)]
    );
    assert_eq!(
        report.children[2].outcome.as_ref().unwrap_err().code(),
        "git_fetch_failed"
    );
}

// ---------------------------------------------------------------------------
// What a marketplace is called
// ---------------------------------------------------------------------------

#[test]
fn a_marketplace_named_with_a_name_of_its_own_is_called_that_and_its_plugins_after_it() {
    let market = anthropic();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let named = marketplace::Request::new(
        Request::git(MARKET)
            .named(Some("anthropic"))
            .through(Arc::clone(&routes) as Arc<dyn Remote>),
        plugins(&["example-skills"]),
    );

    let report = marketplace::gather(&workspace, &ledger, &named).unwrap();

    assert_eq!(report.catalog.name, "anthropic");
    assert_eq!(
        report.children[0].outcome.as_ref().unwrap().name,
        "anthropic.example-skills"
    );
}

#[test]
fn a_catalog_whose_name_names_no_source_is_refused_and_nothing_recorded() {
    let market = repository(&[(
        ".claude-plugin/marketplace.json",
        r#"{"name": "not a name", "plugins": []}"#,
    )]);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let error =
        marketplace::gather(&workspace, &ledger, &request(&routes, Pick::Listing)).unwrap_err();

    assert_eq!(error.code(), "marketplace_name_unusable");
    assert!(ledger.sources().unwrap().is_empty());
    assert!(fs::read_dir(workspace.cache_dir())
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn a_catalogs_name_another_source_has_is_numbered_as_a_name_a_url_gives_is() {
    let market = anthropic();
    let other = repository(&[("skills/x/SKILL.md", &skill("x", "x"))]);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&other)).named(Some("anthropic-agent-skills")),
    )
    .unwrap();

    let report =
        marketplace::gather(&workspace, &ledger, &request(&routes, Pick::Listing)).unwrap();

    assert_eq!(report.catalog.name, "anthropic-agent-skills-2");
}

#[test]
fn a_catalog_naming_its_marketplace_as_its_url_would_keeps_that_name_unnumbered() {
    // `acme/market` is called `acme-market` by its URL until its catalog
    // is read, and the catalog calls it that too.
    let market = repository(&[(
        ".claude-plugin/marketplace.json",
        r#"{"name": "acme-market", "plugins": []}"#,
    )]);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report =
        marketplace::gather(&workspace, &ledger, &request(&routes, Pick::Listing)).unwrap();

    assert_eq!(report.catalog.name, "acme-market");
    assert_eq!(ledger.sources().unwrap()[0].name, "acme-market");
}

#[test]
fn a_repository_holding_no_catalog_is_no_marketplace_and_is_not_recorded() {
    let plain = repository(&[("skills/x/SKILL.md", &skill("x", "x"))]);
    let routes = Arc::new(Routes::default().to(MARKET, &plain));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let error =
        marketplace::gather(&workspace, &ledger, &request(&routes, Pick::Listing)).unwrap_err();

    assert_eq!(error.code(), "marketplace_catalog_missing");
    assert!(ledger.sources().unwrap().is_empty());
}

#[test]
fn a_cancelled_gather_of_a_new_marketplace_leaves_nothing_recorded() {
    let market = anthropic();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let watch = Watch::new();
    watch.cancel();
    let cancelled = marketplace::Request::new(
        Request::git(MARKET)
            .watched_by(watch)
            .through(Arc::clone(&routes) as Arc<dyn Remote>),
        Pick::All,
    );

    let error = marketplace::gather(&workspace, &ledger, &cancelled).unwrap_err();

    assert!(
        matches!(
            error,
            marketplace::MarketplaceError::Gather(GatherError::Cancelled)
        ),
        "{error}"
    );
    assert!(ledger.sources().unwrap().is_empty());
}

#[test]
fn a_plugin_the_catalog_moves_to_another_repository_keeps_its_source() {
    let first = repository(&[("skills/a/SKILL.md", &skill("a", "First"))]);
    let second = repository(&[("skills/a/SKILL.md", &skill("a", "Second"))]);
    let at =
        |url: &str| format!(r#"{{"name": "a", "source": {{"source": "url", "url": "{url}"}}}}"#);
    let market = repository(&[(
        ".claude-plugin/marketplace.json",
        &catalog("m", &[at("https://git.example.com/first.git")]),
    )]);
    let routes = Arc::new(
        Routes::default()
            .to(MARKET, &market)
            .to("https://git.example.com/first.git", &first)
            .to("https://git.example.com/second.git", &second),
    );
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();
    write_files(
        market.path(),
        &[(
            ".claude-plugin/marketplace.json",
            &catalog("m", &[at("https://git.example.com/second.git")]),
        )],
    );
    commit(market.path());

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    let moved = report.children[0].outcome.as_ref().unwrap();
    assert_eq!(moved.name, "m.a");
    assert_eq!(moved.updated.len(), 1, "{moved:?}");
    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 2, "{sources:#?}");
    assert!(sources
        .iter()
        .any(|source| source.url == "https://git.example.com/second.git"));
}

#[test]
fn a_plugin_s_url_naming_a_folder_on_this_machine_is_refused() {
    let elsewhere = repository(&[("skills/secret/SKILL.md", &skill("secret", "x"))]);
    let entry = format!(
        r#"{{"name": "local", "source": {{"source": "url", "url": "{}"}}}}"#,
        url(&elsewhere).replace('\\', "\\\\")
    );
    let market = repository(&[(".claude-plugin/marketplace.json", &catalog("m", &[entry]))]);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    assert!(report.children.is_empty());
    assert_eq!(report.catalog.failures[0].code(), "marketplace_entry_local");
    assert!(ledger.gathered().unwrap().is_empty());
    assert_eq!(report.catalog.failures[0].name(), Some("local"));
}

// ---------------------------------------------------------------------------
// Codex's dialect (#104)
// ---------------------------------------------------------------------------

/// A Codex entry for the plugin in `path`, called `name`, with the policy
/// and the category Codex requires.
fn codex_entry(name: &str, path: &str) -> String {
    format!(
        r#"{{"name": "{name}", "source": {{"source": "local", "path": "{path}"}}, "policy": {{"installation": "AVAILABLE", "authentication": "ON_INSTALL"}}, "category": "Productivity"}}"#
    )
}

/// A Codex plugin's manifest, as openai/plugins writes each, naming its
/// skills' folder, its MCP servers and its app connectors.
fn codex_manifest(name: &str) -> String {
    format!(
        r#"{{"name": "{name}", "version": "1.0.0", "description": "{name} for Codex", "skills": "./skills/", "mcpServers": "./.mcp.json", "apps": "./.app.json", "interface": {{"displayName": "{name}"}}}}"#
    )
}

/// A marketplace as openai/plugins keeps one: its catalog at Codex's
/// place, each plugin a folder of its own under `plugins/` with a
/// `.codex-plugin/plugin.json`, some with skills, one with servers and an
/// app connector alone, and one entry whose path Codex does not resolve.
fn openai() -> TempDir {
    let entries = [
        codex_entry("linear", "./plugins/linear"),
        codex_entry("figma", "./plugins/figma"),
        codex_entry("notion", "./plugins/notion"),
        codex_entry("retired", "plugins/retired"),
    ];
    let catalog = format!(
        r#"{{"name": "openai-curated", "interface": {{"displayName": "OpenAI"}}, "plugins": [{}]}}"#,
        entries.join(",")
    );
    let manifests: Vec<(String, String)> = ["linear", "figma", "notion", "retired"]
        .into_iter()
        .map(|name| {
            (
                format!("plugins/{name}/.codex-plugin/plugin.json"),
                codex_manifest(name),
            )
        })
        .collect();
    let mut files: Vec<(&str, &str)> = manifests
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let linear = skill("linear", "Work with Linear issues");
    let design = skill("figma-design", "Read a Figma design");
    let code = skill("figma-code", "Turn a Figma frame into code");
    let old = skill("old", "Never resolved");
    files.extend([
        (".agents/plugins/marketplace.json", catalog.as_str()),
        ("plugins/linear/skills/linear/SKILL.md", linear.as_str()),
        ("plugins/linear/.mcp.json", r#"{"mcpServers": {}}"#),
        ("plugins/linear/.app.json", r#"{"apps": {}}"#),
        (
            "plugins/figma/skills/figma-design/SKILL.md",
            design.as_str(),
        ),
        ("plugins/figma/skills/figma-code/SKILL.md", code.as_str()),
        ("plugins/notion/.mcp.json", r#"{"mcpServers": {}}"#),
        ("plugins/notion/.app.json", r#"{"apps": {}}"#),
        ("plugins/retired/skills/old/SKILL.md", old.as_str()),
        ("README.md", "Codex plugins\n"),
    ]);
    repository(&files)
}

#[test]
fn a_codex_marketplace_like_openais_lists_its_plugins_and_skips_one_codex_does_not_resolve() {
    let market = openai();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report =
        marketplace::gather(&workspace, &ledger, &request(&routes, Pick::Listing)).unwrap();

    assert_eq!(report.catalog.name, "openai-curated");
    assert_eq!(report.catalog.dialect, marketplace::Dialect::Codex);
    assert_eq!(report.catalog.listed, 4);
    let listed: Vec<(&str, &str, &str)> = report
        .catalog
        .entries
        .iter()
        .map(|entry| {
            (
                entry.name.as_str(),
                entry.plugin.kind(),
                entry.plugin.path(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("linear", "local", "plugins/linear"),
            ("figma", "local", "plugins/figma"),
            ("notion", "local", "plugins/notion"),
        ]
    );
    let failed: Vec<(Option<&str>, &str)> = report
        .catalog
        .failures
        .iter()
        .map(|failure| (failure.name(), failure.code()))
        .collect();
    assert_eq!(failed, [(Some("retired"), "marketplace_entry_unresolved")]);
    assert!(report.children.is_empty());
    assert_eq!(routes.asked(), [MARKET]);
}

#[test]
fn every_plugin_of_a_codex_marketplace_is_gathered_from_its_clone_and_the_unresolved_one_skipped() {
    let market = openai();
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    let gathered: Vec<(&str, usize)> = report
        .children
        .iter()
        .map(|child| {
            (
                child.entry.as_str(),
                child.outcome.as_ref().unwrap().added.len(),
            )
        })
        .collect();
    assert_eq!(gathered, [("linear", 1), ("figma", 2), ("notion", 0)]);
    assert_eq!(
        shelf(&ledger),
        pairs(&[
            ("figma-code", "openai-curated.figma"),
            ("figma-design", "openai-curated.figma"),
            ("linear", "openai-curated.linear"),
        ])
    );
    assert_eq!(
        report.catalog.failures[0].code(),
        "marketplace_entry_unresolved"
    );
    // Its servers are said by the file they are in, and never taken.
    assert_eq!(report.children[0].holds, [Holding::McpServers]);
    assert_eq!(report.children[0].mcp_files, [".mcp.json"]);
    assert_eq!(report.children[2].mcp_files, [".mcp.json"]);
    // Every plugin is in the marketplace's own repository: one fetch.
    assert_eq!(routes.asked(), [MARKET]);
    let sources = ledger.sources().unwrap();
    let parent = sources
        .iter()
        .find(|source| source.name == "openai-curated")
        .unwrap();
    assert_eq!(parent.kind, SourceKind::Marketplace);
    let children: Vec<String> = ledger
        .children(parent.id)
        .unwrap()
        .into_iter()
        .map(|child| child.subdirectory)
        .collect();
    assert_eq!(
        children,
        ["plugins/figma", "plugins/linear", "plugins/notion"]
    );
}

#[test]
fn a_catalog_in_the_other_dialects_shape_is_refused_whichever_place_it_is_at_and_nothing_recorded()
{
    let openai_at_claudes = repository(&[
        (
            ".claude-plugin/marketplace.json",
            &catalog("c", &[codex_entry("linear", "./plugins/linear")]),
        ),
        (
            "plugins/linear/skills/linear/SKILL.md",
            &skill("linear", "Linear"),
        ),
    ]);
    let anthropic_at_codexs = repository(&[
        (
            ".agents/plugins/marketplace.json",
            &catalog("c", &[String::from(r#"{"name": "kit", "source": "./"}"#)]),
        ),
        ("skills/kit/SKILL.md", &skill("kit", "Kit")),
    ]);

    for market in [&openai_at_claudes, &anthropic_at_codexs] {
        let routes = Arc::new(Routes::default().to(MARKET, market));
        let (_dir, workspace) = workspace();
        let ledger = workspace.ledger().unwrap();

        let error =
            marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap_err();

        assert_eq!(error.code(), "marketplace_catalog_other_dialect");
        assert!(ledger.sources().unwrap().is_empty());
        assert!(ledger.gathered().unwrap().is_empty());
    }
}

#[test]
fn a_codex_plugin_is_searched_and_said_as_its_manifest_says() {
    let market = repository(&[
        (
            ".agents/plugins/marketplace.json",
            &catalog("c", &[codex_entry("kit", "./plugins/kit")]),
        ),
        (
            "plugins/kit/.codex-plugin/plugin.json",
            r#"{"name": "kit", "license": "MIT", "skills": "./custom/", "mcpServers": {"x": {"command": "x"}}}"#,
        ),
        // Claude Code's manifest is read only when Codex's is not there.
        (
            "plugins/kit/.claude-plugin/plugin.json",
            r#"{"license": "Apache-2.0", "hooks": "./hooks.json"}"#,
        ),
        (
            "plugins/kit/custom/deploy/SKILL.md",
            &skill("deploy", "Ship it"),
        ),
    ]);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    let child = &report.children[0];
    assert_eq!(child.outcome.as_ref().unwrap().added.len(), 1, "{child:?}");
    // Declared in its manifest, and kept in no file of the two.
    assert_eq!(child.holds, [Holding::McpServers]);
    assert!(child.mcp_files.is_empty());
    let licenses: Vec<Option<String>> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| gathered.license)
        .collect();
    assert_eq!(licenses, [Some(String::from("MIT"))]);
}

#[test]
fn an_agent_plugins_package_a_catalog_lists_is_gathered_as_its_format_says_and_its_servers_said() {
    let market = repository(&[
        (
            ".agents/plugins/marketplace.json",
            &catalog("c", &[codex_entry("kit", "./plugins/kit")]),
        ),
        (
            "plugins/kit/plugin.json",
            r#"{"$schema": "https://example.org/agent-plugins/plugin.schema.json", "name": "kit"}"#,
        ),
        (
            "plugins/kit/skills/deploy/SKILL.md",
            &skill("deploy", "Ship it"),
        ),
        (
            "plugins/kit/skills/team/review/SKILL.md",
            &skill("review", "Nested below a child"),
        ),
        ("plugins/kit/mcp.json", r#"{"mcpServers": {}}"#),
    ]);
    let routes = Arc::new(Routes::default().to(MARKET, &market));
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = marketplace::gather(&workspace, &ledger, &request(&routes, Pick::All)).unwrap();

    assert_eq!(shelf(&ledger), pairs(&[("deploy", "c.kit")]));
    assert_eq!(report.children[0].holds, [Holding::McpServers]);
    assert_eq!(report.children[0].mcp_files, ["mcp.json"]);
}
