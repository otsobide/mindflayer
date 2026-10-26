# AGENTS.md

Guidance for coding agents working in this repository.

Mindflayer manages the skills and rules coding agents read, the way git manages
code. A **mind project** (`.mind/mind.toml`) holds artifacts beside its code; a
**flayer workspace** (`.mindflayer/flayer.toml`) links mind projects, holds
artifacts of its own, gathers skills from git onto a shelf and installs them
into projects. `README.md` is the user-facing surface. `docs/architecture.md`
records why each mechanism is the way it is: read the section for whatever you
are about to change before changing it, and keep it true afterwards. Its
"Decisions" section holds what the owner already chose for the roadmap, and the
one choice still pending: read it before changing a format, installing, MCP,
credentials or the server, and carry a decision out rather than reopen it.

## Commands

The root `Makefile` is what CI runs, so prefer it to raw cargo. `make help`
lists everything.

```bash
make test                 # every suite
make fmt/check            # CI runs this first
make lint                 # clippy --workspace --all-targets
make run ARGS="flayer list"
make core/test            # one app: make <app>/<target>
```

One suite, one module or one test:

```bash
cargo test -p mindflayer-cli --test tui_test                 # apps/cli/tests/tui_test.rs
cargo test -p mindflayer-core --test install_test <substring>
cargo test -p mindflayer-core --lib workspace                # src/workspace_test.rs
cargo test -p mindflayer-server --test commands_test         # apps/server/tests/commands_test.rs
```

`make dev/link`, `make dev/unlink` and `make install` write into `~/.cargo/bin`
(or `BINDIR`), outside the repository. Don't run them unless asked.

## Layout

Three crates in one Cargo workspace, each an app under `apps/` with its own
`Makefile`:

- `apps/core` (`mindflayer-core`): the engine. Projects, workspaces, which
  workspace a command uses (`resolve`), kinds, the catalog, gathering
  (`gix`), the SQLite ledger, installing, keeping installs out of git
  (`exclude`), creating and changing artifacts. Its only I/O is files, and it
  never reads the environment.
- `apps/cli` (`mindflayer-cli`): the binaries `mind` (`src/main.rs`) and
  `flayer` (`src/bin/flayer.rs`), and the TUI. `src/lib.rs` only re-exports
  the public API. The parsers are in `src/parse.rs`. `src/commands.rs`
  dispatches them and asks core what a command acts on, and each family of
  commands has a file under `src/commands/`: `project` (init, link, unlink,
  scan), `workspace` (load, unload), `artifact` (add, edit, rename, remove,
  templates), `read` (list, show, validate), `gather` and `install`.
  `src/render.rs` holds `Outcome`, `Failure` and the table helpers,
  `src/error.rs` holds `CliError`, and `src/homes.rs` decides the default
  home and the agent homes, and gathers `-W` and `$MINDFLAYER_WORKSPACE`
  into a `Lookup`. `src/tui/` and `src/install/` are the two screens.
- `apps/server` (`mindflayer-server`): the local server behind the web
  panel, a library no binary runs yet. `src/backend.rs` is the `Backend`
  trait, `src/server.rs` binds and routes, `src/auth.rs` holds the token's
  cookie and Bearer checks, `src/guard.rs` the Host, Origin and Fetch
  Metadata checks and the CSP, `src/api.rs` the JSON endpoints,
  `src/writer.rs` the thread every run goes through, `src/transcript.rs` what
  the session ran, `src/log.rs` the redacting request log and `src/panel.rs`
  the embedded panel (`apps/web/dist`).

## Invariants that hold the design together

These are easy to break from a single file without noticing.

- **Rules live in core.** The CLI parses, asks core and renders. Name limits,
  front matter, where a kind lives and sort order are all core's, so a future
  front end cannot disagree with this one.
- **Two binaries, one parser.** `flayer <cmd>` is `mind flayer <cmd>`, and
  both reach the same function. `mind add` and `flayer add` (with or without
  `-p`) are one implementation given a different holder
  (`FlayerWorkspace::own` makes the workspace look like a `MindProject`).
- **The TUI only runs commands.** A key produces a `Task`, and
  `Task::commands` turns it into argument lines that are parsed by
  `Cli`/`FlayerCli` and run through `run_with`/`run_flayer_with`, what
  `run`/`run_flayer_cli` call once they have read the environment. A new
  feature is written as a command first; the TUI gains it by producing that
  command's words. Never add a TUI-only code path.
- **Output is built, then printed.** Commands return an `Outcome` (stdout
  text, stderr lines, ok), so tests assert on the exact text a user sees.
- **Failures are collected, not raised.** One unreadable artifact or stale
  registry entry is a `warning:` on stderr and a non-zero exit, never a reason
  to hide the rest. An absence (no `skills/` folder yet) is not a failure.
- **Never overwrite what somebody wrote.**
  - New markers are written from string templates so their comments survive.
    Edits go through `toml_edit` (`edit_array`) and a temp file of their own
    (`replace_file`, named for the run) plus a rename.
  - Creation uses `create_new` or `create_dir`.
  - `rename` rewrites only the `name:` line (`frontmatter::with_key`).
  - `remove` is a dry run without `--yes`.
  - A file somebody else owns is edited only inside Mindflayer's marked
    blocks, every byte outside them kept, and a malformed block is refused,
    not repaired. The blocks: `# mindflayer:begin`/`# mindflayer:end` in
    `$GIT_COMMON_DIR/info/exclude` (`exclude`), where an edit rewrites only its
    own project's `# project <key>` group. A team's `.gitignore` and
    `core.excludesFile` are never edited; the one `.gitignore` Mindflayer
    writes is `.mind/tmp`'s, inside a folder it owns, made with `create_new`.
- **Mindflayer only manages what it installed.** The ledger's record of an
  install — one installation, a target for each copy it wrote, its origin and
  the `hash::tree` of what it wrote, that hash's one definition — is what
  allows overwriting and deleting. An unrecorded artifact is
  `Standing::Foreign` and is never touched. Core's lifecycle changes
  (`lifecycle::add`, `finish_edit`, `rename` and `remove`) drop any record
  they would leave behind and return what they did (`lifecycle::Record`, and
  the `scrub` that opening the ledger ran), which the CLI only renders.
  `install::disown` is private to core, so no front end drops a record on its
  own. A front end adds through `lifecycle::add`, never `create` or
  `create_from`, which leave a stale record in place.
- **Strict when writing, lenient when reading.** A skill's front matter is
  the Agent Skills format. `mind add`, `rename` and `install` refuse a name or
  description that breaks its rules, all through `artifact::strict_issues`
  (what `ValidationIssue::is_strict` picks out), so change the rules there and
  nowhere else; gathering
  takes such a skill with a `warning:` per problem, and `validate` reports
  everything — `flayer validate` without `-p` reads the shelf too — but calls
  an artifact invalid only for what is not `ValidationIssue::is_warning`. Keys the format does not define pass through
  untouched. A rule's front matter is optional (`rule::RuleManifest`): read
  only when its fences hold a YAML mapping, and whatever it leaves unread is a
  warning, never a reason to hide the rule. `mind add --template` refuses that
  same list (`artifact::rule_issues`) for the rule it is about to write.
- **Loaded and gathered content is untrusted.**
  - An installed name must be one folder name (`install::is_folder_name`).
  - Symlinks are followed only inside the source (`copy::tree`'s `within`).
  - A project's artifact directories must stay inside the project.
  - URLs are stored, logged and shown without credentials. `url::redacted`
    is the one function that takes them out: the ledger applies it to every
    URL it writes and reads, `GitError` to what it says, `git::scrub` to a
    clone's config and reflogs. A gather refuses a URL that carries one. A
    command that writes to the ledger opens it with
    `FlayerWorkspace::ledger_to_write`, which scrubs what an older build
    stored first and hands back what it took out to be said; one that only
    reads opens it with `ledger_for_reading`, which scrubs nothing. The
    install screen is a read until it applies, so it scrubs then
    (`install::commit`). Anything new that keeps or prints a URL goes through
    `url::redacted` too.
- **Writers take turns; readers never wait.** Core's writers hold the
  workspace's `.mindflayer/lock` (`FlayerWorkspace::lock`) while they change
  it: `edit_array`, `install::install` and `uninstall`, `gather::gather`, and
  `FlayerWorkspace::ledger`, which opens the ledger to write. A new writer
  takes it the same way, inside core. The lock is not re-entrant: a writer
  never calls another while it holds it, and a front end never takes it.
  Reads (`FlayerWorkspace::open`, `ledger_for_reading`) never take it, and a
  wait is bounded (`lock::WAIT`), ending in a `LockError::Busy` that names the
  holder's pid.
- **A gather never prompts for a credential itself.** Every connection `gix`
  lets a clone configure gets the callback in `gather/credentials.rs`, which
  runs the git-config credential helpers with `gix`'s prompt and askpass
  turned off. A credential none of them has is `GitError::Credentials`, never
  a question on a terminal the TUI owns or a server lacks. Anything new that
  connects to a remote takes the same callback. Two gaps are open questions in
  the architecture doc: what the system ssh asks on the terminal, and `gix`'s
  retry after a remote branch named `HEAD`, a connection the callback cannot
  reach.
- **Formats are versioned.** Each marker carries its own `version`
  (`MIND_FORMAT_VERSION` for `mind.toml`, `FLAYER_FORMAT_VERSION` for
  `flayer.toml`, so bumping one never makes older builds refuse the other; see
  `DIRECTORIES_VERSION` for how a v1 `mind.toml` is still read) and the ledger
  uses SQLite's `user_version`. Anything from a newer version is refused. A
  format change must keep reading old files the way they were written.
  - A ledger schema step is one more block in `Ledger::migrate`, which takes
    the write lock, copies the file to `mindflayer.db.v<old>.bak`
    (`Ledger::back_up`) and runs every step in one transaction. A step fills
    in old rows in its own SQL, not through today's code, and adds a fixture
    of what the version before it wrote
    (`apps/core/tests/fixtures/ledger-v<N>.sql`) for a test to migrate.
- **Nothing is staged where agents look.** An install builds its copy in a
  `copy::Stage` under the project's `.mind/tmp` and renames it into place.
  A temporary beside an installed copy is a skill an agent may load, so never
  write one there. The next install sweeps what an interrupted one left in
  `.mind/tmp`, only when the stage lock says no other run is going, clears the
  `.<name>.mindflayer-new` and `-old` older versions left beside the copy, and
  reports both, even when its own copy then fails.
- **Paths are lexical and portable.** Route arithmetic lives in
  `apps/core/src/paths.rs`. Registry entries are stored relative to the
  workspace with forward slashes, and `link` verifies a route before storing
  it. Directory walking is where Windows differs, which is why CI has a
  macOS/Windows matrix.
- **One function decides the workspace.** Core's `resolve` takes the
  workspace `-W` names, then the one `$MINDFLAYER_WORKSPACE` activates, then
  the first one up the tree, then the default in home, and says which rule
  chose it (`Reason`). A named or activated path that is not
  a workspace is an error, never a fallback, and is never made. A project's
  workspace (`resolve::managing`, `resolve::consulted`) and which project
  `-p` names (`FlayerWorkspace::pick`, `holder`) are core's too: no front end
  walks up, or picks a holder, itself.
- **A change is handed its workspace.** The lifecycle changes
  (`lifecycle::add`, `finish_edit`, `rename`, `remove`) and template lookup
  (`template::templates`, `find`) take it as a parameter and never resolve
  one. The command passes what
  `resolve::consulted` resolves from the project, which walks up only when
  nothing is named or activated; a walk misses a workspace that links the
  project from beside it, and `-W` is how that one is reached.
- **Home and the chosen workspace come from the front end.** `Lookup::given`
  takes `--home`, then `$MINDFLAYER_HOME`, then the user's home, and `-W`,
  then `$MINDFLAYER_WORKSPACE`, which is not read once `-W` is given; empty
  means none, and an empty `-W` keeps the activated one out too. Core gets
  them as `Inputs` and never reads the environment. Only commands that write
  may create `~/.mindflayer` (`Intent::Write`). Reads resolve with
  `Intent::Read`, which falls back to the workspace `init` would make without
  making it; a read opens `mindflayer.db` only through `ledger_for_reading`,
  which never creates it either. The TUI hands its own `Lookup` to every line
  it runs (`tui::execute`), so none of them reads the environment again.
- **So do the agent homes.** `homes::agent_homes` takes each from its option
  (`--claude-config-dir`, `--codex-home`, `--opencode-config-dir`,
  `--agents-dir`), then `$CLAUDE_CONFIG_DIR`, `$CODEX_HOME` or
  `$OPENCODE_CONFIG_DIR`, then `~/.claude`, `~/.codex`, `~/.config/opencode`
  or `~/.agents`; empty means none. Those folder names are core's
  (`AgentHomes::defaults_under`), and only the user's home is the CLI's to
  find. Core takes the answer as `AgentHomes` and never resolves one. No
  command calls `agent_homes` yet, and the first that does also makes
  `tui::execute` hand each command the TUI's own, as it hands them its
  `Lookup`: a command the TUI runs carries none of the four options, so it
  would decide them again from the environment.
- **The server holds no rules.** `apps/server` carries JSON between the
  browser and a `Backend`, whose `Query`, `Task` and `Outcome` are associated
  types, and depends on no mindflayer crate (`tests/crate_test.rs` checks it
  with `cargo metadata`); a front end depends on it, as the CLI will (#67),
  never the other way. A check, path rule or naming rule written there is one
  the panel could disagree with the CLI about: it belongs in core, reached
  through the backend. Queries and previews run on tokio's blocking pool, and
  every run on the one writer thread, in the order it arrived. It binds
  127.0.0.1 only, answers only a Host naming it on its port as `127.0.0.1`,
  `localhost` or `[::1]`, refuses an Origin other than its own, answers under
  `/api` only a request that carries the launch token (in the cookie
  `mf_<port>`, or as Bearer), and takes a change by cookie only with
  `Sec-Fetch-Site: same-origin`, `X-Mindflayer: 1` and `application/json`.
  Every answer carries the CSP and `nosniff`; there is no CORS layer, and the
  token is never logged.
- **`Kind` is a closed enum.** To add a kind, follow "Adding a third kind" in
  the architecture doc; the compiler's exhaustive matches are the checklist.

## Tests

- Integration suites are `apps/<app>/tests/<area>_test.rs`, with shared
  helpers in `tests/common/mod.rs` (`use common::*;`). Unit tests sit in
  `src/<module>_test.rs`, wired in with
  `#[cfg(test)] #[path = "<module>_test.rs"] mod tests;`.
- No test reads the real home, the real agent homes or a workspace the
  developer's shell activated, which `HOME`, `MINDFLAYER_HOME`,
  `MINDFLAYER_WORKSPACE`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and
  `OPENCODE_CONFIG_DIR` lead to. In-process tests pin options: CLI tests call
  `common::mind` / `common::flayer`, which parse through the real parsers and
  append `-C <dir>`, `--home=`, `--workspace=` and the four agent-home
  options empty (`common::PINNED`), so no command they run can reach or
  create the real home workspace, use an activated one or read a real agent
  folder. A command a TUI test runs gets only the TUI's own `Lookup`, the one
  the test chose, and none of the agent-home options (see "So do the agent
  homes"). A test that spawns a binary (`common::spawn`) clears those
  variables (`common::CLEARED`), sets only the ones it is about, and sets
  `HOME` (`USERPROFILE` on Windows) to a temp dir. Not to empty: the standard
  library then falls back to the account's real home. Keep it that way.
- Git sources are built in-process with `gix` (`common::repository`): no
  network and no `git` binary.
- The server's suites run against a fake backend with types of its own
  (`common::Fake`), never the CLI's. They bind 127.0.0.1 on port 0 and send
  requests through the router in process (`common::send`), or over loopback,
  and serve a fixture panel (`tests/panel`), so they need no Node. Every
  request names the server in Host, and `Auth::Cookie` sends what the panel's
  page would; a test of a guard changes one header of that
  (`common::request`, then `common::deliver`).
- Screens are tested without a terminal. Drive `state` with key events, or
  call `tui::step`, and render `ui` into `ratatui::backend::TestBackend`. Only
  the event loops in `tui.rs` and `install.rs` go untested, so keep logic out
  of them.
- Fixtures use `tempfile::TempDir`. Test names are sentences
  (`init_flayer_is_the_same_as_init`).

## Workflow

- Gitflow is enforced by required checks. PRs into `dev` come from
  `feature/*`, `hotfix/*` or `release/*` (or `main`, for a back-merge). PRs
  into `main` come only from `dev`. PRs are merged with merge commits.
- CI runs `fmt and clippy`, then `test (core)`, then `test (cli)` and
  `test (server)`, and builds each app after its tests pass. macOS and
  Windows run on pushes to `dev`/`main`, on PRs from `dev`, `main` or
  `release/*`, and on PRs labelled `full-matrix`.
- Commit messages: `<area>: <imperative summary>`, where area is `core`,
  `cli`, `server`, `docs`, `build`, `ci` or `chore` (`core, cli:` when a
  change spans two crates).
- When behaviour changes, update the README (its examples are literal output,
  so re-run them) and `docs/architecture.md`, including its "Open questions".

## Style

Comments and doc comments explain *why* a choice was made, often naming the
alternative that was rejected. They rarely narrate what the code does. Match
that density and voice in code and docs alike.
