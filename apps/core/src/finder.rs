//! The skill directories `gather search` asks (D-15, #108), each a
//! [`Finder`]: a service that helps find a skill and names where it is, a
//! git repository and a folder in it, and is never a source itself. What
//! is taken is gathered from that repository, as `gather git` gathers it,
//! so a directory whose API changes or goes away breaks nothing already on
//! the shelf, and its failure is a warning.
//!
//! Every finder is off until the workspace's marker lists it
//! ([`enabled`]), and one that is not is never asked: two of them are
//! services whose API is undocumented and whose terms a person reads
//! before using them (D-15), and none of them is asked on a person's
//! behalf without their say.
//!
//! Each is asked through the [`Client`] its caller hands over ([`Search`]),
//! so whether answers are kept is the caller's, and a test's transport a
//! fake. What a finder answers is somebody else's JSON, read leniently, as
//! a registry's is: an answer whose shape this build does not read is a
//! failure of that finder alone, beside what the others found
//! ([`Failure::Unreadable`]); a result that does not read, or whose
//! address does not resolve to a git source `gather git` would take, is
//! dropped and said ([`Failure::Dropped`]); and every string a front end
//! is handed is made printable first.

use std::borrow::Cow;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::code::{codes, Said};
use crate::gather::outside;
use crate::http::{fetchable, on_this_machine, Cache, Client, HttpError, Request, Unsent};
use crate::text::printable;
use crate::url::{self, carries_credentials};
use crate::value::Redacted;
use crate::WeaverWorkspace;

mod claude_plugins;
mod github;
mod skills_sh;

/// How many results each finder is asked for: its first page, a screen's
/// worth or two. A search is where a person starts looking, and one that
/// asks for more of each than anyone reads makes every finder's limit run
/// out sooner, GitHub's above all.
pub const PER_FINDER: usize = 20;

/// A skill directory a search can ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Finder {
    /// GitHub's code search, for a `SKILL.md`: asked only with a token,
    /// which the API wants for a code search.
    Github,
    /// skills.sh's `/api/search`: unstable, since it is undocumented.
    SkillsSh,
    /// api.claude-plugins.dev's search of the skills it lists: unstable,
    /// since it is undocumented.
    ClaudePlugins,
}

impl Finder {
    /// Every finder, in the order a search lists what they found.
    pub const ALL: [Finder; 3] = [Finder::Github, Finder::SkillsSh, Finder::ClaudePlugins];

    /// The word the marker and a command line name it by: the service's
    /// own name, so what is listed in `weaver.toml` is what a person
    /// reading its terms looked up.
    pub fn name(self) -> &'static str {
        match self {
            Finder::Github => "github",
            Finder::SkillsSh => "skills.sh",
            Finder::ClaudePlugins => "claude-plugins.dev",
        }
    }

    /// The finder `name` names, if any.
    pub fn named(name: &str) -> Option<Finder> {
        Finder::ALL.into_iter().find(|finder| finder.name() == name)
    }

    /// Whether what it answers may change shape without a word: an API
    /// nobody documents, read as it answers today, best-effort.
    pub fn is_unstable(self) -> bool {
        match self {
            Finder::Github => false,
            Finder::SkillsSh | Finder::ClaudePlugins => true,
        }
    }

    /// Whether it is asked only with a token: GitHub's code search, which
    /// answers no request without one.
    pub fn needs_token(self) -> bool {
        match self {
            Finder::Github => true,
            Finder::SkillsSh | Finder::ClaudePlugins => false,
        }
    }

    /// Where it is asked unless a caller names another address that
    /// speaks the same API.
    pub fn address(self) -> &'static str {
        match self {
            Finder::Github => "https://api.github.com",
            Finder::SkillsSh => "https://skills.sh",
            Finder::ClaudePlugins => "https://api.claude-plugins.dev",
        }
    }
}

impl fmt::Display for Finder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// By its name, the word a command line takes, rather than derived from
/// the variant, as a kind and an agent go out.
impl Serialize for Finder {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

/// `FINDER=URL`, a finder by its name and the address it is asked at
/// instead of its own, split at the first `=`, which no finder's name
/// holds: what `--finder-url` takes, and a front end's line carries.
pub fn located(text: &str) -> Result<(Finder, String), SearchError> {
    text.split_once('=')
        .and_then(|(name, url)| Some((Finder::named(name)?, url.to_owned())))
        .ok_or_else(|| SearchError::Unlocated {
            given: Redacted::new(text),
        })
}

/// The finders a workspace's marker lists, and what it lists that is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enabled {
    /// Each finder listed, in [`Finder::ALL`]'s order, once.
    pub finders: Vec<Finder>,
    /// Each name listed that this build knows no finder by.
    pub unknown: Vec<UnknownFinder>,
    /// The marker they are listed in, which what is said of them names.
    pub config: PathBuf,
}

/// The finders `workspace`'s marker lists in `finders`.
///
/// A name this build does not know is passed over and said, rather than
/// refused: a later build may know a finder this one does not, and a
/// marker it wrote is still read. One that is not made yet, the default
/// before a write makes it, lists none.
pub fn enabled(workspace: &WeaverWorkspace) -> Enabled {
    let config = workspace.config_path();
    let listed = &workspace.config().finders;
    let finders = Finder::ALL
        .into_iter()
        .filter(|finder| listed.iter().any(|name| name == finder.name()))
        .collect();
    let mut unknown: Vec<UnknownFinder> = Vec::new();
    for name in listed {
        let said = printable(name);
        if Finder::named(name).is_none() && !unknown.iter().any(|known| known.name == said) {
            unknown.push(UnknownFinder {
                name: said,
                config: config.clone(),
            });
        }
    }
    Enabled {
        finders,
        unknown,
        config,
    }
}

impl Enabled {
    /// The finders a search asks: those `named`, each of which is to be
    /// enabled, or every one enabled when none is named.
    ///
    /// One named and not enabled is refused rather than asked or passed
    /// over: it was asked for by name, and asking it is what listing it
    /// would allow, which is the person's to do (D-15).
    pub fn asked(&self, named: &[Finder]) -> Result<Vec<Finder>, SearchError> {
        if let Some(finder) = named.iter().find(|finder| !self.finders.contains(finder)) {
            return Err(SearchError::NotEnabled {
                finder: *finder,
                config: self.config.clone(),
            });
        }
        let asked: Vec<Finder> = if named.is_empty() {
            self.finders.clone()
        } else {
            Finder::ALL
                .into_iter()
                .filter(|finder| named.contains(finder))
                .collect()
        };
        if asked.is_empty() {
            return Err(SearchError::NoneEnabled {
                config: self.config.clone(),
            });
        }
        Ok(asked)
    }
}

/// Why a search asked nothing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SearchError {
    /// Nothing to look for, which no finder answers.
    #[error("say what to look for: an empty query finds nothing")]
    Empty,
    /// The marker lists no finder, and none was named.
    #[error(
        "no finder is enabled, so nothing was searched: list one in `finders` in {} (github, \
         skills.sh, claude-plugins.dev; the last two are unstable and undocumented, so read their \
         terms first)",
        .config.display()
    )]
    NoneEnabled { config: PathBuf },
    /// A finder's address that names no finder ([`located`]).
    #[error(
        "`{given}` is no finder's address: give a finder's name (github, skills.sh, \
         claude-plugins.dev), `=` and the address it is asked at"
    )]
    Unlocated { given: Redacted },
    /// `--take` naming a result the search did not list.
    #[error("there is no result {asked} to take: {}", results(*found))]
    NoSuchResult { asked: usize, found: usize },
    /// A finder named that the marker does not list.
    #[error(
        "{finder} is not enabled, so it was not asked: list it in `finders` in {} to search it",
        .config.display()
    )]
    NotEnabled { finder: Finder, config: PathBuf },
}

codes!(SearchError {
    Self::Empty => "search_query_empty",
    Self::NoSuchResult { .. } => "search_result_missing",
    Self::Unlocated { .. } => "finder_url_invalid",
    Self::NoneEnabled { .. } => "finder_none_enabled",
    Self::NotEnabled { .. } => "finder_not_enabled",
});

/// How many results a search found, as a refusal to take another says it.
fn results(found: usize) -> String {
    match found {
        0 => String::from("the search found none"),
        1 => String::from("the search found 1"),
        found => format!("the search found {found}"),
    }
}

/// A name the marker lists among its finders that this build knows no
/// finder by: passed over, and said.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "{}: `{name}` is among its finders, and this build knows none by that name, so it is passed over",
    .config.display()
)]
pub struct UnknownFinder {
    /// The name, printable.
    pub name: String,
    pub config: PathBuf,
}

codes!(UnknownFinder => "finder_unknown");

/// A credential a front end hands over for one search, GitHub's token:
/// never kept, printed or serialized, so it has no `Display` and no
/// `Serialize`, and says only that it is one when debugged.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// `value` as a token, unless it is empty, which is none.
    pub fn new(value: impl Into<String>) -> Option<Token> {
        let value = value.into();
        (!value.is_empty()).then_some(Token(value))
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<credential>)")
    }
}

/// Finders asked through a client.
#[derive(Debug)]
pub struct Search<'a> {
    client: &'a Client<'a>,
    /// The address each finder is asked at, where a caller named another
    /// than its own ([`Finder::address`]).
    at: Vec<(Finder, String)>,
    token: Option<Token>,
}

impl<'a> Search<'a> {
    /// Each finder at its own address, through `client`, with no token.
    pub fn new(client: &'a Client<'a>) -> Self {
        Self {
            client,
            at: Vec::new(),
            token: None,
        }
    }

    /// Ask `finder` at `base` rather than its own address: one of a
    /// person's own that speaks the same API, a GitHub Enterprise server's
    /// (`https://ghe.example/api/v3`), or a test's.
    pub fn at(mut self, finder: Finder, base: impl Into<String>) -> Self {
        self.at.retain(|(each, _)| *each != finder);
        self.at.push((finder, base.into()));
        self
    }

    /// Send `token` to the finders that need one, and to no other.
    pub fn with_token(mut self, token: Option<Token>) -> Self {
        self.token = token;
        self
    }

    /// Where `finder` is asked.
    pub fn base(&self, finder: Finder) -> &str {
        self.at
            .iter()
            .find(|(each, _)| *each == finder)
            .map_or(finder.address(), |(_, base)| base.as_str())
    }

    /// What each of `finders` finds for `query`, in [`Finder::ALL`]'s order,
    /// each in the order it ranked them, a result two finders name once.
    ///
    /// One that needs a token, asked without one, is skipped and sent
    /// nothing. One that fails, is rate limited or answers in a shape this
    /// build does not read is a failure beside what the others found,
    /// never a reason to hide it.
    pub fn run(&self, query: &str, finders: &[Finder]) -> Result<Found, SearchError> {
        if query.trim().is_empty() {
            return Err(SearchError::Empty);
        }
        let mut found = Found {
            hits: Vec::new(),
            skipped: Vec::new(),
            failures: Vec::new(),
            warnings: Vec::new(),
        };
        let asked = Finder::ALL
            .into_iter()
            .filter(|finder| finders.contains(finder));
        for finder in asked {
            if finder.needs_token() && self.token.is_none() {
                found.skipped.push(finder);
                continue;
            }
            match self.ask(finder, query, &mut found.warnings) {
                Ok(listed) => {
                    let local = on_this_machine(self.base(finder));
                    for (at, entry) in listed.into_iter().enumerate() {
                        match entry.and_then(|listed| listed.resolved(local)) {
                            Ok(hit) => found.add(finder, hit),
                            Err((name, why)) => found.failures.push(Failure::Dropped {
                                finder,
                                at: at + 1,
                                name,
                                why: Redacted::new(&why),
                            }),
                        }
                    }
                }
                Err(failure) => found.failures.push(failure),
            }
        }
        Ok(found)
    }

    /// What `finder` answers for `query`, each result read on its own, or
    /// why there is nothing to read.
    fn ask(
        &self,
        finder: Finder,
        query: &str,
        warnings: &mut Vec<HttpError>,
    ) -> Result<Vec<Entry>, Failure> {
        let failed = |error| Failure::Failed { finder, error };
        let base = self.base(finder);
        let request = match finder {
            Finder::Github => github::request(base, query, self.token.as_ref()),
            Finder::SkillsSh => skills_sh::request(base, query),
            Finder::ClaudePlugins => claude_plugins::request(base, query),
        }
        .map_err(failed)?;
        let fetched = self.client.get(&request).map_err(|failed| {
            warnings.extend(failed.warnings);
            Failure::Failed {
                finder,
                error: failed.error,
            }
        })?;
        let value = fetched.json::<serde_json::Value>();
        warnings.extend(fetched.warnings);
        let read = match finder {
            Finder::Github => github::read,
            Finder::SkillsSh => skills_sh::read,
            Finder::ClaudePlugins => claude_plugins::read,
        };
        read(value.map_err(failed)?).map_err(|detail| Failure::Unreadable {
            finder,
            detail: Redacted::new(&detail),
        })
    }
}

/// What a search for `query` of `finders`, each at the address `at`
/// names or its own, found the last time it was made through a client
/// keeping its answers in `cache`, read from the cache alone: nothing is
/// sent, however long ago each answer was kept. `None` when the cache keeps
/// no finder's answer to it.
///
/// What a front end that ran the search as a command line reads what the
/// line found from, as the TUI does once the line is over, rather than
/// reading the line's output back or asking the finders again on the
/// thread that draws, as [`crate::registry::kept`] reads a registry's.
/// No token is sent, and none was kept: what GitHub's code search answered
/// a token is no answer kept, so it is skipped here, and only the finders
/// that need none are read again. A finder whose answer is not kept is
/// left out, as one that was never asked.
pub fn kept(
    cache: Cache,
    query: &str,
    finders: &[Finder],
    at: &[(Finder, String)],
) -> Option<Found> {
    let client = Client::new(&Unsent).with_cache(cache.fresh_for(Duration::MAX));
    let search = at
        .iter()
        .fold(Search::new(&client), |search, (finder, base)| {
            search.at(*finder, base.clone())
        });
    let mut found = search.run(query, finders).ok()?;
    let unkept = |failure: &Failure| {
        matches!(
            failure,
            Failure::Failed {
                error: HttpError::Unavailable { .. },
                ..
            }
        )
    };
    let missing: Vec<Finder> = found
        .failures
        .iter()
        .filter(|failure| unkept(failure))
        .map(Failure::finder)
        .collect();
    found.failures.retain(|failure| !unkept(failure));
    let read = finders
        .iter()
        .any(|finder| !finder.needs_token() && !missing.contains(finder));
    read.then_some(found)
}

/// The URL of `segments` below `base`, with `query`, each value escaped as
/// the value it is: a finder's address may have a path of its own, as a
/// GitHub Enterprise server's API does (`/api/v3`).
fn below(base: &str, segments: &[&str], query: &[(&str, &str)]) -> Result<String, HttpError> {
    let mut url = fetchable(base)?;
    url.set_query(None);
    url.set_fragment(None);
    url.path_segments_mut()
        .map_err(|()| HttpError::Url {
            url: Redacted::new(base),
            detail: String::from("no path can be put below it"),
        })?
        .pop_if_empty()
        .extend(segments);
    url.query_pairs_mut().extend_pairs(query);
    Ok(url.to_string())
}

/// A request for JSON at `url`, as `accept` names it.
fn request(url: &str, accept: &str) -> Request {
    Request::get(url).header("Accept", accept)
}

/// One result of a finder's answer, read or not: what is wrong with one
/// that is not, and its name when it said one.
type Entry = Result<Listed, (Option<String>, String)>;

/// Each of `items` read as `T` and then as a result, by `listed`, what
/// serde said of one that does not read made printable, and one that does
/// not said by the string under `label`, which names it in the answer.
fn entries<T: serde::de::DeserializeOwned>(
    items: Vec<serde_json::Value>,
    label: &str,
    listed: impl Fn(T) -> Result<Listed, String>,
) -> Vec<Entry> {
    items
        .into_iter()
        .map(|item| {
            let name = item
                .get(label)
                .and_then(serde_json::Value::as_str)
                .map(printable);
            serde_json::from_value::<T>(item)
                .map_err(|error| printable(&error.to_string()))
                .and_then(&listed)
                .map_err(|why| (name, why))
        })
        .collect()
}

/// A count as a finder gives one: a whole number, or nothing known, so a
/// count given in another form is no reason to drop what it counts.
fn count(value: Option<&serde_json::Value>) -> Option<u64> {
    value.and_then(serde_json::Value::as_u64)
}

/// One result as a finder listed it, before it is held to what a git
/// source is.
#[derive(Debug, Default)]
struct Listed {
    name: Option<String>,
    /// The repository: a URL, GitHub's `owner/repo`, or a forge's tree URL,
    /// which names a branch and a folder too.
    address: Option<String>,
    /// The folder in it, where the finder says it apart from the address.
    path: Option<String>,
    installs: Option<u64>,
    stars: Option<u64>,
}

impl Listed {
    /// The result, if its address resolves to a git source `gather git`
    /// would take, from a finder on this machine when `local`; its name
    /// and why it does not otherwise.
    ///
    /// A folder on this machine is taken from a finder on this machine
    /// alone, one of a person's own or a test's: a service elsewhere naming
    /// one would have a gather put on the shelf whatever a folder here
    /// holds, which no directory is asked to reach, as a marketplace's
    /// catalog may not. A credential in an address is dropped without
    /// being said, and a folder that climbs out of the repository is
    /// refused here as `gather git --path` refuses it.
    fn resolved(self, local: bool) -> Result<Hit, (Option<String>, String)> {
        let name = self.name.as_deref().map(printable);
        let refused = |why: &str| (name.clone(), why.to_owned());
        let Some(typed) = self.address.as_deref().filter(|typed| !typed.is_empty()) else {
            return Err(refused("it names no repository"));
        };
        if printable(typed) != typed {
            return Err(refused(
                "its repository has a control character or a space at either end",
            ));
        }
        if carries_credentials(typed) {
            return Err(refused("its repository's address carries a credential"));
        }
        let address = url::address(typed)
            .map_err(|_| refused("its repository's address names a folder that cannot be read"))?;
        let remote =
            url::is_remote(&address.url) || matches!(url::fetched(&address.url), Cow::Owned(_));
        if !remote && !local {
            return Err(refused(
                "it names a folder on this machine, which only a finder on this machine may",
            ));
        }
        let path = match self.path.or(address.path) {
            Some(path) => {
                let path = path.trim_matches('/');
                if printable(path) != path {
                    return Err(refused(
                        "its folder has a control character or a space at either end",
                    ));
                }
                if outside(path) {
                    return Err(refused("its folder climbs out of the repository"));
                }
                (!path.is_empty() && path != ".").then(|| path.to_owned())
            }
            None => None,
        };
        let reference = address.reference.filter(|reference| !reference.is_empty());
        if reference
            .as_deref()
            .is_some_and(|reference| printable(reference) != reference)
        {
            return Err(refused(
                "its branch has a control character or a space at either end",
            ));
        }
        let named = name
            .clone()
            .filter(|name| !name.is_empty())
            .or_else(|| {
                path.as_deref()
                    .and_then(|path| path.rsplit('/').next())
                    .map(str::to_owned)
            })
            .or_else(|| url::repository(&address.url));
        let Some(named) = named else {
            return Err(refused("it has no name"));
        };
        Ok(Hit {
            name: named,
            repository: Redacted::new(&url::canonical(&address.url)),
            path,
            reference,
            installs: self.installs,
            stars: self.stars,
            finders: Vec::new(),
        })
    }
}

/// What a search found, and what it could not.
#[derive(Debug)]
pub struct Found {
    /// Each result, once, in [`Finder::ALL`]'s order and each finder's own.
    pub hits: Vec<Hit>,
    /// Each finder asked that needs a token, and was given none: sent
    /// nothing.
    pub skipped: Vec<Finder>,
    /// Each finder that failed, and each result dropped, beside the rest.
    pub failures: Vec<Failure>,
    /// What went wrong with the cache on the way, which did not stop it.
    pub warnings: Vec<HttpError>,
}

impl Found {
    /// The result listed `number`, counting from 1, as a search lists them
    /// and `--take` names one: the one a front end gathers, by its
    /// repository and its folder.
    pub fn take(&self, number: usize) -> Result<&Hit, SearchError> {
        number
            .checked_sub(1)
            .and_then(|at| self.hits.get(at))
            .ok_or(SearchError::NoSuchResult {
                asked: number,
                found: self.hits.len(),
            })
    }

    /// `hit`, found by `finder`, listed once however many finders name it:
    /// by its repository, as [`url::canonical`] spells every spelling of
    /// one, and its folder in it. A second finder's is said beside the
    /// first's, with a count the first did not give.
    fn add(&mut self, finder: Finder, hit: Hit) {
        match self
            .hits
            .iter_mut()
            .find(|each| each.repository == hit.repository && each.path == hit.path)
        {
            Some(each) => {
                if !each.finders.contains(&finder) {
                    each.finders.push(finder);
                }
                each.installs = each.installs.or(hit.installs);
                each.stars = each.stars.or(hit.stars);
            }
            None => self.hits.push(Hit {
                finders: vec![finder],
                ..hit
            }),
        }
    }
}

/// By hand, as a registry's listing is: what went wrong is errors, each
/// its code and its message ([`Said`]).
impl Serialize for Found {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut found = serializer.serialize_struct("Found", 4)?;
        found.serialize_field("hits", &self.hits)?;
        found.serialize_field("skipped", &self.skipped)?;
        found.serialize_field(
            "failures",
            &self
                .failures
                .iter()
                .map(|failure| Said::of(failure.code(), failure))
                .collect::<Vec<_>>(),
        )?;
        found.serialize_field(
            "warnings",
            &self
                .warnings
                .iter()
                .map(|warning| Said::of(warning.code(), warning))
                .collect::<Vec<_>>(),
        )?;
        found.end()
    }
}

/// One skill a search found: where it is, as `gather git` takes it, and
/// what the finders say of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    /// What it is called, printable: the name a finder gave it, or its
    /// folder's, or its repository's for one at the root.
    pub name: String,
    /// The repository it is in, as [`url::canonical`] spells it.
    pub repository: Redacted,
    /// The folder in the repository it is in, none for the root or where
    /// the finder does not say, which is gathered from the root.
    pub path: Option<String>,
    /// The branch the finder named, none for the repository's default.
    pub reference: Option<String>,
    /// How many times it was installed, where a finder counts that.
    pub installs: Option<u64>,
    /// How many stars its repository has, where a finder counts them.
    pub stars: Option<u64>,
    /// Each finder that found it, in the order they were asked.
    pub finders: Vec<Finder>,
}

/// A finder that failed, or a result of one dropped, said beside what the
/// rest found.
#[derive(Debug, Error)]
pub enum Failure {
    /// Its answer did not come back: refused, timed out, rate limited.
    #[error("{finder}: {error}")]
    Failed { finder: Finder, error: HttpError },
    /// Its answer came back, in a shape this build does not read: an
    /// undocumented API that changed, and nothing it found is listed.
    #[error(
        "{finder}: its answer is not in the shape this build reads, so nothing it found is \
         listed: {detail}"
    )]
    Unreadable { finder: Finder, detail: Redacted },
    /// One result that does not read, or does not name a git source.
    #[error("{finder}: result {at}{}: {why}, so it is not listed", .name.as_deref().map(|name| format!(" (`{name}`)")).unwrap_or_default())]
    Dropped {
        finder: Finder,
        /// Where it is among the finder's results, counting from 1, as a
        /// search numbers what it lists and `--take` names one.
        at: usize,
        /// Its name, when it said one, printable.
        name: Option<String>,
        why: Redacted,
    },
}

impl Failure {
    /// The finder it is of.
    pub fn finder(&self) -> Finder {
        match self {
            Failure::Failed { finder, .. }
            | Failure::Unreadable { finder, .. }
            | Failure::Dropped { finder, .. } => *finder,
        }
    }
}

codes!(Failure {
    Self::Unreadable { .. } => "finder_answer_unreadable",
    Self::Dropped { .. } => "finder_result_dropped",
} carried {
    Self::Failed { error, .. } => error,
});

#[cfg(test)]
#[path = "finder_test.rs"]
mod tests;
