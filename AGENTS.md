# AGENTS.md

Guidance for coding agents working in this repository.

Mindflayer manages the skills and rules coding agents read, the way git manages
code. A **mind project** (`.mind/settings.json`) holds artifacts beside its
code; a **flayer workspace** (`.mindflayer/flayer.toml`) links mind projects,
holds artifacts of its own, gathers skills from git onto a shelf and installs
them into projects. `README.md` is the user-facing surface.
`docs/architecture.md` records why each mechanism is the way it is: read the
section for whatever you are about to change before changing it, and keep it
true afterwards. Its "Decisions" section holds what the owner already chose for
the roadmap, and the one choice still pending: read it before changing a
format, installing, MCP, credentials or the server, and carry a decision out
rather than reopen it. `docs/agents.md` records where Claude Code, Codex and
OpenCode read skills and rules, each fact verified on the binary or docs-only
with a fallback: an agent path in code cites it, and a change that disagrees
with it starts there.

## Commands

The root `Makefile` is what CI runs, so prefer it to raw cargo. `make help`
lists everything.

```bash
make test                 # every Rust suite; the panel's is make web/test
make fmt/check            # CI runs this first
make lint                 # no em dash in a tracked file, then clippy --workspace --all-targets
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

The panel (`apps/web`) needs Node 24 and pnpm at the version `package.json`
pins in `packageManager`. The targets that run a tool install from the
lockfile first, and `PNPM` names pnpm where corepack has not put it on the
PATH:

```bash
make web/test             # vitest: apps/web/tests/<area>.test.ts
make web/lint             # ESLint, its style included; make web/fmt fixes that
make web/build            # vue-tsc, then apps/web/dist, which the server embeds
make web/types            # vue-tsc alone
make web/test PNPM='npx --yes pnpm@12.15.0'
pnpm --dir apps/web exec vitest run tests/shell.test.ts      # one suite
```

`make test`, `make build`, `make fmt` and `make lint` are Rust only: `web` is
in the root Makefile's `NODE_APPS`, outside the aggregates, until CI installs
Node (#71). A change to the panel runs its own targets. The one exception is
`make lint`'s em dash check (`make lint/em-dash`), which reads every tracked
file, the panel's included.

`make dev/link`, `make dev/unlink` and `make install` write into `~/.cargo/bin`
(or `BINDIR`), outside the repository. Don't run them unless asked.

`scripts/agent-probe/probe.sh` runs the real coding agents against their APIs
with your own login, in a temp home under `target/`, to check where they read
skills and rules. It is never run by CI or `make test`; don't run it unless
asked.

## Layout

Three crates in one Cargo workspace, and the panel they serve, each an app
under `apps/` with its own `Makefile`:

- `apps/core` (`mindflayer-core`): the engine. Projects, a project's
  settings (`settings`, the v3 marker), workspaces, which
  workspace a command uses (`resolve`), the shell code that activates one
  (`activate`), kinds, the agents and where each kind goes for them
  (`agent`), the catalog, gathering (`gix`), the SQLite ledger,
  installing, keeping installs out of git (`exclude`), changing a project's
  agents or share mode and carrying its installs along (`reapply`), what
  each project holds, from where and for which agents (`status`), creating
  and changing artifacts, and moving a workspace's own into its library and
  a project from its `mind.toml` to settings (`migrate`). Its only I/O is
  files, and it never reads the environment.
- `apps/cli` (`mindflayer-cli`): the binaries `mind` (`src/main.rs`) and
  `flayer` (`src/bin/flayer.rs`), and the TUI. `src/lib.rs` only re-exports
  the public API. The parsers are in `src/parse.rs`. `src/commands.rs`
  dispatches them and asks core what a command acts on, and each family of
  commands has a file under `src/commands/`: `project` (init, link, unlink,
  scan), `workspace` (load, unload), `activate` (activate, which),
  `artifact` (add, edit, rename, remove, templates), `read` (list, show,
  validate), `status`, `settings` (agents, share), `gather`, `install` and
  `migrate` (migrate, at both levels). `src/render.rs` holds `Outcome`, `Failure`, the table helpers
  and the envelope every `--json` prints (`json`),
  `src/error.rs` holds `CliError`, and `src/homes.rs` decides the default
  home and the agent homes, and gathers them with `-W` and
  `$MINDFLAYER_WORKSPACE` into a `Lookup`. `src/tui/` and `src/install/` are
  the two screens.
- `apps/server` (`mindflayer-server`): the local server behind the web
  panel, a library no binary runs yet. `src/backend.rs` is the `Backend`
  trait, `src/server.rs` binds and routes, `src/auth.rs` holds the token's
  cookie and Bearer checks, `src/guard.rs` the Host, Origin and Fetch
  Metadata checks and the CSP, `src/api.rs` the JSON endpoints,
  `src/writer.rs` the thread every run goes through, `src/transcript.rs` what
  the session ran, `src/log.rs` the redacting request log and `src/panel.rs`
  the embedded panel (`apps/web/dist`).
- `apps/web` (`mindflayer-web`): the web panel, in Vue and Nuxt UI on Vite,
  with pnpm. `src/main.ts` mounts what `src/panel.ts` installs (router,
  Pinia, Vue Query, Nuxt UI, the transport), which the shell test installs
  too. `src/api/transport.ts` is the `Transport` interface and `useTransport`,
  and `src/api/http.ts` its HTTP implementation. `src/pages/` holds one page
  per route (file routes), `src/components/` the frame (`AppSidebar`,
  `AppHeader`, `AppLogo`), `src/stores/` Pinia's UI state, `src/query.ts`
  the Vue Query client, `src/theme.ts` the palette, `src/color-mode.ts` the
  colour mode, and `src/overrides/` what replaces two pieces of the
  libraries for the CSP. `src/*.d.ts` are written by the Vite plugins and
  committed.

Beside them, `examples/` is content and not an app: a flayer workspace whose
own two skills and two rules anyone can `flayer load` to try Mindflayer
without gathering. `apps/cli/tests/examples_test.rs` holds every example to
`validate` with no warning and finds the README's "Try it with the examples"
output in what the commands print, so changing an example's summary line
means changing the README too.

Outside `apps/`, `scripts/agent-probe/` holds the agent probe: `probe.sh`
isolates a run and checks the real home is left alone, and `<agent>.sh` holds
one agent's checks (only `claude.sh` so far).

## Invariants that hold the design together

These are easy to break from a single file without noticing.

- **Rules live in core.** The CLI parses, asks core and renders. Name limits,
  front matter, where a kind lives, where it goes for each agent, where each
  agent reads it and sort order are all core's, so a future front end cannot
  disagree with this one. No front end spells an agent's folder or calls
  `TargetTable::standard`: which table applies is the project's, in core, the
  standard one until #40 reads an override from its settings, so code that
  needs one asks the project (`MindProject::targets`), as discovery and an
  install (`install::targets`) do. Where an agent reads (`Agent::reads`) is
  kept apart from where it is sent a copy: OpenCode reads `.claude/skills` and
  is sent nothing there. Which folders a project is read from is core's too
  (`Catalog::folders`).
- **Two binaries, one parser.** `flayer <cmd>` is `mind flayer <cmd>`, and
  both reach the same function. `mind add` and `flayer add` (with or without
  `-p`) are one implementation given a different holder
  (`FlayerWorkspace::own` makes the workspace look like a `MindProject`).
- **The TUI only runs commands.** A key produces a `Task`, and
  `Task::commands` turns it into argument lines that a `Runner` hands to
  `execute`, which parses them with `Cli`/`FlayerCli` and runs them through
  `run_with`/`run_flayer_with` with the screen's `Lookup` and a
  `gather::Watch`: what the binaries' `run_watched`/`run_flayer_cli_watched`
  call once they have read the environment. A test's stand-in `Runner` is
  handed the same words. A new feature is written as a command first; the
  TUI gains it by producing that command's words. Never add a TUI-only code
  path. A gather's lines run on a thread of their own (`Task::runs_apart`),
  still through the same parsers: the thread changes where they run, not
  what they are.
- **A gather is watched, and stops only before it places.** Its `Request`
  carries a `gather::Watch`, core's progress callback and cancel flag; no gix
  type reaches a front end. A cancel is honoured until the shelf starts to
  change, and leaves the shelf and the ledger as they were, so a write to
  either before that last check is undone on a cancel, as registering the
  source is.
- **Output is built, then printed.** Commands return an `Outcome` (stdout
  text, stderr lines, ok), so tests assert on the exact text a user sees.
- **Reading commands print JSON in one envelope, and its names are a
  contract.** A reading command's `--json` prints `render::json`'s
  `{"version": 1, "data": ..., "warnings": [...]}`, never a shape of its own,
  with what it would have said on stderr as `warning:` lines in `warnings`
  and the exit status its text has. `data` is a type core builds and sorts,
  deriving `Serialize` with its fields named in snake_case; a kind, an agent
  or a state goes out as the word a command line takes for it
  (`Kind::slug`, `Agent::name`, `State::slug`), serialized by hand, never
  derived from the variant. A name once printed is a contract: dropping or
  renaming one, or changing what it holds, moves `ENVELOPE_VERSION`, and
  adding one does not. A test reads it back with `common::enveloped`, into
  structs of its own that refuse a field they do not name.
- **Failures are collected, not raised.** One unreadable artifact or stale
  registry entry is a `warning:` on stderr (in the envelope, under `--json`)
  and a non-zero exit, never a reason to hide the rest. An absence (no
  `skills/` folder yet) is not a failure.
- **Never overwrite what somebody wrote.**
  - New markers are written from string templates so their comments survive.
    Edits go through `toml_edit` (`edit_array`, or `edit_marker_held` for a
    writer that holds the lock already) and a temp file of their own
    (`replace_file`, named for the run) plus a rename.
  - A project's `.mind/settings.json`, which has no comments to keep, is
    serialized (`Settings::text`) and made with `create_new`. It is changed
    only through `MindProject::change_settings`, which reads the file again
    and edits each key whose value changed, `directories` an entry at a time,
    through `config_entry`, as an agent's config file is (below), then
    `replace_file`: a key this build does not know, the key order and a
    hand-made layout survive, and nothing is written when nothing changed.
    Its agents and share mode change through `reapply::apply` (`mind agents`,
    `mind share`), which carries what is installed along with them.
  - Creation uses `create_new` or `create_dir`.
  - `rename` rewrites only the `name:` line (`frontmatter::with_key`).
  - `remove`, `migrate` at either level, `agents` and `share` are dry runs
    without `--yes`.
    A migration of the library moves nothing a project holds, refuses as a
    whole, and keeps the marker it rewrites as `flayer.toml.v<version>`
    (`create_new`). A project's (`migrate::project`) writes its settings
    with `create_new`, keeps its `mind.toml` as
    `.mind/tmp/mind.toml.v<version>`, never runs git (it says `git rm -r
    --cached .mind` instead), and puts back every step it took when a later
    one fails.
  - A file somebody else owns is edited only inside Mindflayer's marked
    blocks or, in an agent's config file, which can carry no marks, one entry
    at a time (next bullet). Every byte outside them is kept, and a malformed
    block is refused, not repaired. The blocks:
    `# mindflayer:begin`/`# mindflayer:end` in
    `$GIT_COMMON_DIR/info/exclude` (`exclude`), where an edit rewrites only its
    own project's `# project <key>` group: an install writes one entry per
    copy for a local project, keyed by its id, before the copies land, and
    never opens the file for a shared project or one a `mind.toml` marks. A
    project turning shared has its whole group forgotten (`exclude::forget`),
    and one turning local an entry written for each copy in place. A team's
    `.gitignore` and `core.excludesFile` are never edited; the `.gitignore`s
    Mindflayer writes are `.mind/tmp`'s and a local project's `.mind`'s own
    (`MIND_IGNORE`), each inside a folder it owns, made with `create_new`,
    and the second is removed, when the project turns shared, only while it
    still holds `MIND_IGNORE`.
  - An entry in a config file somebody else owns (`.mcp.json`,
    `opencode.json`, `.codex/config.toml`) is changed one at a time through
    `config_entry`: JSON and JSONC entries are edited through jsonc-parser's
    CST (an insert spliced into the text where it says) and TOML through
    `toml_edit`, never parsed and serialized again, so every byte outside the
    entry is kept. What an edit wrote is read back and refused unless only
    the entry changed, and a file that does not parse is refused, never
    rewritten. A changed file goes through `replace_bytes_unresolved`,
    keeping its permissions; a new one is made with `create_new`. The module
    knows no agent: the file, the key path and the entry are the caller's.
  - `flayer init` refuses a directory for the workspace's own that overlaps
    the shelf or the cache, which are reserved for gathering
    (`gathered_overlap`). `flayer init` and the workspace's own artifacts
    never put an `AGENTS.md`, a `CLAUDE.md` or a kind's folder in the
    workspace's directory; they live in `.mindflayer`. An install into a
    project at the workspace's root is that project's (D-03).
- **Mindflayer only manages what it installed.** The ledger's record of an
  install (one installation, a target for each copy it wrote, its origin and
  the `hash::tree` of what it wrote, that hash's one definition) is what
  allows overwriting and deleting. An install writes a copy in every target
  `install::targets` gives (each folder the project's agents read, or the
  one a `mind.toml` names), built from one tree and put in place together or
  not at all (`copy::Stage::place`). Standing is by the path each record
  names (`Ledger::copies`), never by name: anything unrecorded in any target
  is `Standing::Foreign`, refuses the whole install with nothing written in
  any target, and is never touched. An uninstall removes the copies its
  record names and nothing else. No copy holds a skill's top-level
  `.claude-plugin` (`install::LeftOut`), and none lands inside an agent's
  own folder (`InstallError::UserScope`). A v3 project is listed from its
  agents' folders too, where an unrecorded copy outside the folder it
  authors in is `Provenance::Foreign` (by the path a record names,
  `Catalog::with_records`), and a lifecycle change looks only where the
  project authors (`Catalog::discover_authored`), so it never reaches one.
  Core's lifecycle changes
  (`lifecycle::add`, `finish_edit`, `rename` and `remove`) drop any record
  they would leave behind, only that of the copy the change reached
  (`Ledger::uninstalled_copy`: a v3 project whose authored folder is also a
  target keeps the record of its other copies, and a local one stops
  excluding that copy's place), and return what they did (`lifecycle::Record`, and the `scrub` that opening the ledger ran),
  which the CLI only renders. A change of a project's agents
  (`reapply::apply`) moves only what the ledger records: a new folder gets
  a copy of one whose `hash::tree` is the one recorded, never one over
  somebody's, and a dropped folder loses a recorded copy only while its
  hash is the one recorded; one changed since is left there, and its record
  and exclude entry dropped. A project's move to settings
  (`migrate::project`) moves its installs the same way, through
  `reapply::retarget`, and holds a record from before hashes to its offer
  (D-04). Being made once, it is refused when the workspace `mind migrate`
  consults cannot be read, never made as if that one recorded nothing.
  `install::disown` is private to core, so no front end drops a record on its
  own. A front end adds through `lifecycle::add`, never `create` or
  `create_from`, which leave a stale record in place.
- **Strict when writing, lenient when reading.** A skill's front matter is
  the Agent Skills format. `mind add`, `rename` and `install` refuse a name or
  description that breaks its rules, all through `artifact::strict_issues`
  (what `ValidationIssue::is_strict` picks out), so change the rules there and
  nowhere else; gathering
  takes such a skill with a `warning:` per problem, and `validate` reports
  everything (`flayer validate` without `-p` reads the shelf too) but calls
  an artifact invalid only for what is not `ValidationIssue::is_warning`. Keys the format does not define pass through
  untouched. A rule's front matter is optional (`rule::RuleManifest`): read
  only when its fences hold a YAML mapping, and whatever it leaves unread is a
  warning, never a reason to hide the rule. `mind add --template` refuses that
  same list (`artifact::rule_issues`) for the rule it is about to write.
- **Loaded and gathered content is untrusted.**
  - An installed name must be one folder name (`install::is_folder_name`).
  - Symlinks are followed only inside the source (`copy::tree`'s `within`).
  - A project's artifact directories must stay inside the project.
  - A config file is named by a route of folder names from the project, and
    a symlink anywhere on it, the file included, is refused, never followed
    (`config_entry::ConfigFile`): the route is looked at when the file is
    read and again just before it is written, and the write is never
    resolved. A folder made a link between that last look and the rename is
    the one window left (architecture.md, open questions).
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
  it: `edit_array`, `install::install` and `uninstall`, `gather::gather`,
  `gather::scrub`, which `FlayerWorkspace::ledger_to_write` runs, when it has
  clones to rewrite, `migrate::library`, from its plan to its rewrite of
  the marker, `migrate::project`, from before it reads the project again
  until its ledger is written, and `reapply::apply`, from before it reads a
  project's settings until their change and every copy it carries along are
  written. A new
  writer takes it the same way, inside core; `config_entry::ConfigFile` and
  `MindProject::change_settings` take none, since the writers that call them
  hold it. A second workspace linking the same project is kept out by
  neither lock, an open question in `docs/architecture.md`.
  Opening the ledger takes no lock: a schema migration keeps itself apart
  in its own transaction. The lock is not re-entrant: a writer never calls
  another while it holds it, and a front end never takes it. Reads
  (`FlayerWorkspace::open`, `ledger_for_reading`) never take it. A wait is
  bounded (`lock::WAIT`), ending in a `LockError::Busy` that names the
  holder's pid, except a record drop's (`install::disown`), which follows a
  change already made.
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
  (`MIND_FORMAT_VERSION` for a project's, `FLAYER_FORMAT_VERSION` for
  `flayer.toml`, so bumping one never makes older builds refuse the other)
  and the ledger uses SQLite's `user_version`. A project's marker is
  `mind.toml` up to `MIND_TOML_VERSION`, which only `MindProject::init` still
  writes, for tests of such a project, and `settings.json` from
  `SETTINGS_VERSION`, which `mind init` writes (`MindProject::init_asked`,
  which fills in each folder it was not told about); either makes a
  directory a project (`MindProject::is_marked`), and the settings win when
  both are there. See `DIRECTORIES_VERSION` for how a v1 `mind.toml` is
  still read, and `SETTINGS_VERSION` for why a kind a settings file does not
  name has no folder (`directory_for` is `None`). Anything from a newer
  version is refused. A format change must keep reading old files the way
  they were written, and move one forward only when asked, never on a read
  (`flayer migrate --library`, `mind migrate`).
  - A ledger schema step is one more block in `Ledger::migrate`, which takes
    the write lock, copies the file to `mindflayer.db.v<old>.bak`
    (`Ledger::back_up`) and runs every step in one transaction. A step fills
    in old rows in its own SQL, not through today's code, and adds a fixture
    of what the version before it wrote
    (`apps/core/tests/fixtures/ledger-v<N>.sql`) for a test to migrate.
- **Nothing is staged where agents look.** An install builds its copies in a
  `copy::Stage` under the project's `.mind/tmp` and renames them into place.
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
  find. Core takes the answer as `AgentHomes` and never resolves one.
  `Lookup::given` decides them once per command line, into the `Lookup`
  (`Lookup::agents`), and install is the first to take them, to refuse a
  copy inside one. `tui::execute` hands each line the TUI's own with the
  rest of its `Lookup`, and so does its install screen: a command the TUI
  runs carries none of the four options, so it would decide them again from
  the environment.
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
- **The panel holds no rules either, and talks only through its
  transport.** It asks and draws: nothing a query answers is worked out in
  TypeScript. Every request goes through the `Transport` from
  `useTransport()`, and only `src/api/http.ts` may use `fetch`,
  `EventSource`, `WebSocket` or `XMLHttpRequest` (an ESLint rule, tested in
  `tests/lint.test.ts`). Authentication is the transport's: relative URLs,
  the `HttpOnly` cookie, the `X-Mindflayer: 1` header. What the server
  answers belongs in Vue Query; Pinia holds UI state only.
- **The panel needs nothing the CSP refuses.** No URL naming another host,
  no inline `<script>` or `<style>`, no `data:` URL: fonts are Fontsource
  files, icons are bundled Lucide (`i-lucide-<name>` in a `.vue` file) drawn
  by Iconify's offline build (`src/overrides/iconify.ts`), the palette is
  declared in `main.css` while Nuxt UI's colours plugin is a stub
  (`src/overrides/colors.ts`), and the colour mode switches by class
  (`src/color-mode.ts`). `tests/dist.test.ts` builds the panel and checks
  it; `tests/shell.test.ts` fails on any `<style>` element added at run time.
- **Activating prints, and writes nothing.** `flayer activate` prints core's
  code (`activate::script`), and nothing on stdout when it fails, so `eval`
  runs nothing. Its shell comes from `--shell`, then `$SHELL`, read by the
  front end. The code undoes only what it did, through `flayer-deactivate`
  and never `deactivate`, and a workspace's name reaches a prompt only
  through `activate::prompt`.
- **`Kind` and `Agent` are closed enums.** To add a kind, follow "Adding a
  third kind" in the architecture doc, and to add an agent, "Adding an agent",
  which starts in `docs/agents.md`. Either way the compiler's exhaustive
  matches are the checklist: the target table's `standard`, and `read` behind
  `Agent::reads`, match every kind and agent with no wildcard, so neither
  compiles until it says where it goes and where it reads.
  "Adding an agent" also lists what no match covers: `Agent::ALL`, and an
  agent home's field, option and place in `common::PINNED` and `CLEARED`.

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
  the test chose, agent homes included (none, from `Lookup::default`), and
  none of the agent-home options (see "So do the agent homes"). A test that
  spawns a binary (`common::spawn`) clears those variables
  (`common::CLEARED`), sets only the ones it is about, and sets
  `HOME` (`USERPROFILE` on Windows) to a temp dir. Not to empty: the standard
  library then falls back to the account's real home. Keep it that way.
- Git sources are built in-process with `gix` (`common::repository`): no
  network and no `git` binary. A CLI fixture's project is a repository given
  `mind init` (`common::new_project`), so a local install keeps its copies out
  of git rather than saying on stderr that nothing does, and where an install
  puts its copies is asked of core (`common::copies`). A test about what a
  project an older Mindflayer made does, a copy installed in the folder it
  authors in, makes one with `common::older_project`. What git would list is
  asked of gix too (`common::untracked`).
- Tests that run a shell (`apps/cli/tests/shells_test.rs`, Linux only) start
  it with no startup file and an environment holding only a `PATH`, a
  temporary `HOME` and what the test sets, so no `MINDFLAYER_*` variable or
  real shell configuration gets in. A shell, or a python3 to make a
  virtualenv with, that is not installed is skipped and said on stderr,
  except under `$CI`, where it fails the test: the `test (cli)` job
  installs zsh so that both shells run there.
- The server's suites run against a fake backend with types of its own
  (`common::Fake`), never the CLI's. They bind 127.0.0.1 on port 0 and send
  requests through the router in process (`common::send`), or over loopback,
  and serve a fixture panel (`tests/panel`), so they need no Node. Every
  request names the server in Host, and `Auth::Cookie` sends what the panel's
  page would; a test of a guard changes one header of that
  (`common::request`, then `common::deliver`).
- The panel's suites are `apps/web/tests/<area>.test.ts`, in Vitest. The
  components run in happy-dom, mounted through `createPanel` with a fake
  `Transport` and a memory history, so they never reach a server.
  `dist.test.ts` builds the panel in a process of its own, into a temporary
  folder under `node_modules`, and leaves `dist/` alone. `licences.test.ts`
  reads every installed package's licence: a new one outside its list of
  GPL-3.0-compatible licences fails it.
- Screens are tested without a terminal. Drive `state` with key events, or
  call `tui::step`, and render `ui` into `ratatui::backend::TestBackend`. Only
  the event loops in `tui.rs` and `install.rs` go untested, so keep logic out
  of them. A task that runs apart is started by `tui::step` and taken in by
  `tui::settle` or `tui::wait`; to make one slow, stand a `Runner` in for the
  real command lines (`App::runner`).
- Fixtures use `tempfile::TempDir`. Test names are sentences
  (`init_flayer_is_the_same_as_init`).

## Workflow

- Gitflow is enforced by required checks. PRs into `dev` come from
  `feature/*`, `hotfix/*` or `release/*` (or `main`, for a back-merge). PRs
  into `main` come only from `dev`. PRs are merged with merge commits.
- CI runs `fmt and clippy`, then `test (core)`, then `test (cli)` and
  `test (server)`, and builds each app after its tests pass. Development
  targets Linux only for now: macOS and Windows run only by hand
  (`workflow_dispatch`) or on PRs labelled `full-matrix`, until the
  compatibility pass (#154) puts them back on pushes to `dev`/`main` and PRs
  from `dev`, `main` or `release/*`.
- Commit messages: `<area>: <imperative summary>`, where area is `core`,
  `cli`, `server`, `web`, `docs`, `build`, `ci` or `chore` (`core, cli:` when
  a change spans two apps).
- When behaviour changes, update the README (its examples are literal output,
  so re-run them) and `docs/architecture.md`, including its "Open questions".

## Style

Comments and doc comments explain *why* a choice was made, often naming the
alternative that was rejected. They rarely narrate what the code does. Match
that density and voice in code and docs alike.

No em dash (U+2014), in docs, comments, messages or commit messages: the owner
finds it reads as machine-written. Write a comma, a colon, parentheses or a
new sentence instead, never a hyphen or a spaced en dash in its place.
`make lint` fails on one in any tracked file; a commit message is yours to
check.
