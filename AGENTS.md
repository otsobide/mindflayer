# AGENTS.md

Guidance for coding agents working in this repository.

Mindflayer manages the skills and rules coding agents read, the way git manages
code. A **mind project** (`.mind/settings.toml`) holds artifacts beside its
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
file, the panel's included. The other way round, `make install` and `make
cli/release` build the panel before the binaries that embed it, so they need
Node, and `make web/build` touches `apps/server/src/panel.rs` so the server
compiles again and takes the panel in.

`make dev/link`, `make dev/unlink` and `make install` write into `~/.cargo/bin`
(or `BINDIR`), outside the repository. Don't run them unless asked.

`scripts/agent-probe/probe.sh` runs the real coding agents against their APIs
with your own login, in a temp home under `target/`, to check where they read
skills and rules. It is never run by CI or `make test`; don't run it unless
asked.

## Layout

Three crates in one Cargo workspace, and the panel they serve, each an app
under `apps/` with its own `Makefile`:

- `apps/core` (`mindflayer-core`): the engine. Projects, a project's settings
  (`settings`, the marker from v3 on), workspaces, which workspace a command
  uses (`resolve`), the shell code that activates one (`activate`), kinds, the
  agents and where each kind goes for them (`agent`), the catalog, gathering
  (`gix`), the SQLite ledger, installing, keeping installs out of git
  (`exclude`), a project's rules rendered into the block of its `AGENTS.md`
  (`agents_md`) and into Claude's rules folder (`claude_rules`), whether
  Claude reads that block and the `@AGENTS.md` import in a project's
  `CLAUDE.md` that makes it (`claude_md`), installing a rule through those
  three, or as a copy into a project a `mind.toml` marks (`install::rules`),
  changing a project's agents or share mode and carrying its installs along
  (`reapply`), what each project holds, from where and for which agents
  (`status`), finding a project that moved again by its id and giving a
  copy made with its `.mind` one of its own (`relink`), the workspaces this
  machine knows, a list the default
  workspace keeps (`known`), creating and changing artifacts, and moving a
  workspace's own into its library and a project from its `mind.toml` or
  `settings.json` to a `settings.toml` (`migrate`), bringing what was
  installed up to date with where it came from (`update`), installing one
  artifact into several projects at once, or taking it, or whatever nothing
  offers any more, out of them (`assign`), removing one of the
  workspace's own from every project it links before the library
  (`everywhere`), and bringing a skill somebody keeps in a project's agents'
  folders into the library, its copies recorded as installed from there
  (`adopt`), the stable code each of its errors gives (`code`), the
  snapshot every front end reads a workspace through, and the part of it
  `list --json` prints (`view`), one file of an artifact read as text for
  the panel (`contents`), which paths and entries a request from outside
  may name (`policy`), and what `show` and `validate` say of each
  artifact (`reading`). Its only I/O is files, and it never reads the
  environment.
- `apps/cli` (`mindflayer-cli`): the binaries `mind` (`src/main.rs`) and
  `flayer` (`src/bin/flayer.rs`), and the TUI. `src/lib.rs` only re-exports
  the public API. The parsers are in `src/parse.rs`. `src/commands.rs`
  dispatches them and asks core what a command acts on, and each family of
  commands has a file under `src/commands/`: `project` (init, link, unlink,
  relink, scan), `workspace` (load, unload), `activate` (activate, which), `known`
  (workspaces, workspaces forget), `artifact` (add, edit, rename, remove,
  templates), `read` (list, show, validate), `status`, `settings` (agents,
  share, targets), `gather`, `install`, `update` and `migrate` (each at both
  levels), and `adopt` (flayer adopt). `src/render.rs` holds `Outcome`,
  `Failure` and `Warning` (a stderr line, with its code beside it), the
  table helpers and the envelope every `--json` prints (`json`, and
  `as_json`, which lays a text outcome out in it),
  `src/error.rs` holds `CliError` and `CODES`, every code the CLI gives of
  its own, and `src/homes.rs` decides the default home and the agent
  homes, and gathers them with `-W` and `$MINDFLAYER_WORKSPACE` into a
  `Lookup`. `src/task.rs` holds `Task`, work a front end asks for as the
  command lines it is, every front end's rather than the TUI's alone, and
  `Refusal`, what a front end with no terminal does not run.
  `commands::run_line` runs one line a front end built, for the TUI and the
  panel alike. `src/admit.rs` holds a task sent from outside to the path
  policy (`Admission`), and `src/backend.rs` is the panel's backend
  (`PanelBackend`, the server's `Backend` with the CLI's `Query`, `Task`
  and `Outcome`), which no binary runs yet (#68).
  `src/tui/` and `src/install/` are the two screens.
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
  standard one overridden for each kind its settings name `targets` for, with
  every folder an install may use (`misplaced_target`), so code that needs
  one asks the project (`MindProject::targets`), as discovery and an install
  (`install::targets`) do. Which agents read none of a project's targets is
  core's too (`ProjectStatus::unreached`), and so is which read a rule it
  received nowhere (`ProjectStatus::missed`). Which kind a name typed bare
  is, to install or take out, is core's (`install::offered_typed`,
  `uninstall_typed`), and so is what a render of rules warns of
  (`Rendered::warnings_of`). Where an agent reads
  (`Agent::reads`) is kept apart from where it is sent a copy: OpenCode reads
  `.claude/skills` and is sent nothing there. Which folders a project is read
  from is core's too (`Catalog::folders`), and so are the names at a
  project's root that show an agent works there (`Agent::reads_at_root`) and
  the order `flayer scan --suggest` ranks what it finds in (`scan::Order`),
  in which the minds screen and the link form show what is not linked,
  without sorting it.
- **Two binaries, one parser.** `flayer <cmd>` is `mind flayer <cmd>`, and
  both reach the same function. `mind add` and `flayer add` (with or without
  `-p`) are one implementation given a different holder
  (`FlayerWorkspace::own` makes the workspace look like a `MindProject`).
- **The TUI only runs commands.** A key produces a `Task` (`src/task.rs`,
  shared by every front end that runs commands, the panel's backend too),
  and `Task::commands` turns it into argument lines that a `Runner` hands to
  `execute`, which runs each through `commands::run_line`: it parses them
  with `Cli`/`FlayerCli` and runs them through `run_with`/`run_flayer_with`
  with the screen's `Lookup` and a `gather::Watch`, what the binaries'
  `run_watched`/`run_flayer_cli_watched` call once they have read the
  environment. The panel's backend runs its lines through `run_line` too. A test's stand-in `Runner` is
  handed the same words. A new feature is written as a command first; the
  TUI gains it by producing that command's words. Never add a TUI-only code
  path. A gather's lines run on a thread of their own (`Task::runs_apart`),
  still through the same parsers: the thread changes where they run, not
  what they are. The install screen applies the same way, in the TUI or
  opened by `flayer install` (`install::apply`): its marks are the
  `flayer install` and `flayer uninstall` lines they amount to
  (`Screen::task`), each run whatever came of the others
  (`Task::chained`), so its transcript runs again. What a front end with
  no terminal does not run is one list, `task::Refusal`, read off the
  parsed command by a match with no wildcard (`Refusal::of_mind`,
  `of_flayer`), so a new command says whether it is refused before it
  compiles; a task is refused for its lines (`Task::refusal`). The TUI
  refuses only its screens.
- **A gather is watched, and stops only before it places.** Its `Request`
  carries a `gather::Watch`, core's progress callback and cancel flag; no gix
  type reaches a front end. A cancel is honoured until the shelf starts to
  change, and leaves the shelf and the ledger as they were, so a write to
  either before that last check is undone on a cancel, as registering the
  source is.
- **Output is built, then printed.** Commands return an `Outcome` (stdout
  text, stderr lines, ok), so tests assert on the exact text a user sees.
  Each stderr line, and each of a `Failure`'s warnings, is a
  `render::Warning`: the text a terminal shows, word for word, and beside
  it the code of what it says (`code()` of the core error, refusal or
  failure it renders, or `CliError::code`), `None` for a hint or a note. A
  line built from a value with a code takes that code; never a code spelled
  by hand for a line built from text.
- **Reading commands print JSON in one envelope, and its names are a
  contract.** A reading command's `--json` (`list`, `show`, `validate`,
  `templates` and `status` at both levels, `workspaces`, `scan` and `gather
  list`) prints `render::json`'s `{"version": 2, "data": ..., "warnings":
  [...], "coded_warnings": [...]}`, never a shape of its own, with what its
  text says on stderr as `warning:` lines in `warnings`, the same lines
  with their codes in `coded_warnings`, and the exit status its text has;
  those lines are still said on stderr too, a `hint:` there alone. One that
  reads as the text does lays out the text's own outcome
  (`render::as_json`), so that every warning it says, one added on the way
  out included, reaches the envelope, and its exit status is the text's by
  construction. `data` is a type core builds and sorts,
  deriving `Serialize` with its fields named in snake_case; a kind, an agent
  or a state goes out as the word a command line takes for it
  (`Kind::slug`, `Agent::name`, `State::slug`, `Standing::slug`), serialized
  by hand, never derived from the variant. The read types #61 named
  serialize so (`docs/architecture.md`, "The read types serialize", lists
  them and the older ones that do not yet), and every new read type
  derives `Serialize` from the start, written by hand only where a derive
  would print the wrong thing (a word like those, a type holding more than
  a reader is given, an error, which goes out as its `code` and message
  through `code::Said`), with a test that pins its JSON name by name. A
  path goes out as text whatever bytes it holds (`paths::lossy`), and a
  URL only through `url::redacted`. A name once printed is a
  contract: dropping or renaming one, or changing what it holds, moves
  `ENVELOPE_VERSION`, and adding one does not. A test reads it back with
  `common::enveloped`, into structs of its own that refuse a field they do
  not name.
- **Front ends read a workspace through the snapshot.** What a screen shows
  of a workspace is core's `view::Snapshot` (`Snapshot::read`, or
  `Snapshot::of` for a front end that resolved the workspace itself), and of
  a project no workspace links `view::Project::read`: built and sorted in
  core from the read types, never put together from the reports in a front
  end. It is a read: the ledger only through `ledger_for_reading`, the
  default workspace never made, nothing scrubbed, and no scan, which is a
  query of its own (a test counts scans with `scan::scans`, a test build's).
  Its names are a contract under `view::FORMAT`, as a report's are under
  `ENVELOPE_VERSION`, and every URL in it, an error's message included,
  goes out through `url::redacted`. What a front end reads beside it is
  only what it cannot hold: a scan, and which workspace a project's own
  lines consult (`docs/architecture.md`, "The read model").
- **Every error has a stable code.** Each public error type in core,
  `ValidationIssue`, and `install::Refused`, why an install or an uninstall
  wrote nothing (`Installed::refused`, `Removed::refused`, and for a copy
  an update or a change of agents leaves, `update::Step::refused` and
  `reapply::Why::refused`), is given `code()` with `code::codes!`, which
  writes it as a match with no wildcard, so a new variant does not compile
  without one, and lists the codes it gives in `CODES` from the same arms.
  Each arm names one variant, and the macro refuses a wildcard or an
  or-pattern, which would give
  several one code and list it once. A code is unique across core, not
  only its type, never renamed nor moved to another condition, and named
  for the condition in snake_case. A variant
  that only carries another error of core's, adding where or what came of
  it, takes that error's code (`carried`). A new error type joins
  `code::ALL`; `code_test.rs` holds no two codes alike and finds a type
  that derives `Error` and never joined.
- **Failures are collected, not raised.** One unreadable artifact or stale
  registry entry is a `warning:` on stderr (in the envelope too, under
  `--json`) and a non-zero exit, never a reason to hide the rest. An absence (no
  `skills/` folder yet) is not a failure.
- **Never overwrite what somebody wrote.**
  - New markers are written from string templates so their comments survive:
    `flayer.toml`, and a project's `.mind/settings.toml` (`Settings::text`),
    made with `create_new`. Edits go through `toml_edit` and one `rewrite`,
    which reads the file again, hands its text to the edit and writes back
    only what changed through a temp file of its own (`replace_file`, named
    for the run) plus a rename: `edit_array`, or `edit_marker_held` for a
    writer that holds the lock already, and `MindProject::change_settings`,
    which edits each key whose value changed where it stands
    (`settings::edited`), `directories` and `targets` an entry at a time,
    keeping the comment on a changed value's line: comments, a key this
    build does not know, the key order and a hand-made layout survive, and
    nothing is written when nothing changed. Only a `settings.toml` is
    changed (`MindProject::changeable`): a `settings.json`, read as it was
    written, is refused with a pointer to `mind migrate`, and `config_entry`
    is never used to edit a project's settings.
    Its agents, targets and share mode change through `reapply::apply`
    (`mind agents`, `mind targets`, `mind share`), which carries what is
    installed along with them. Its id is refused there, naming the project
    for good, and changes only through `MindProject::change_id`, which
    `relink::new_id` calls for a copy made with its `.mind` (`mind init
    --new-id`).
  - Creation uses `create_new` or `create_dir`.
  - `rename` rewrites only the `name:` line (`frontmatter::with_key`).
  - `remove` (`flayer remove --everywhere` too), `migrate` at either level,
    `agents`, `share`, `targets`, `update` and `flayer adopt` are dry runs
    without `--yes`.
    A migration of the library moves nothing a project holds, refuses as a
    whole, and keeps the marker it rewrites as `flayer.toml.v<version>`
    (`create_new`). A project's (`migrate::project`) writes its
    `settings.toml` with `create_new`, keeps the `mind.toml` or
    `settings.json` it replaces as `.mind/tmp/<name>.v<version>`, never runs
    git (it says `git rm -r --cached .mind` instead), and puts back every
    step it took when a later one fails.
  - A file somebody else owns is edited only inside Mindflayer's marked
    blocks or, in an agent's config file, which can carry no marks, one entry
    at a time (next bullet). Every byte outside them is kept, and a malformed
    block is refused, not repaired. The blocks:
    `# mindflayer:begin`/`# mindflayer:end` in
    `$GIT_COMMON_DIR/info/exclude` (`exclude`), where an edit rewrites only its
    own project's `# project <key>` group: an install writes one entry per
    copy for a local project, keyed by its id, before the copies land, and
    a render of Claude's rule files one per file, and neither opens the file
    for a shared project or one a `mind.toml` marks. A
    project turning shared has its whole group forgotten (`exclude::forget`),
    and one turning local an entry written for each copy in place. No entry
    is written for a copy git tracks (`install::untracked`), by an install,
    an adoption or a project turning local: it would keep none of the copy
    out, and hide what is added to it later. A
    relinked project's lines, and a copy's given a new id, are carried to its
    new place or key (`exclude::carry`), never left as a stale group. A team's
    `.gitignore` and `core.excludesFile` are never edited; the `.gitignore`s
    Mindflayer writes are `.mind/tmp`'s and a local project's `.mind`'s own
    (`MIND_IGNORE`), each inside a folder it owns, made with `create_new`,
    and the second is removed, when the project turns shared, only while it
    still holds `MIND_IGNORE`. And `<!-- mindflayer:begin -->` and
    `<!-- mindflayer:end -->` in a project's root `AGENTS.md` (`agents_md`),
    a section a rule under `<!-- mindflayer:rule <route> -->`: a render
    replaces the block whole, adds it after what the file holds, an empty
    line apart, or drops it, that line with it, and gives the file back as
    it was, in the file's own line endings. A
    marker written any other way, a rule's marker outside the block and one
    route twice are refused like a broken block. A missing `AGENTS.md` is
    made with `create_new`, and only then said to be created; one that is
    there goes through `replace_bytes`, written through a link only to a
    file inside the project; and the file is taken away only when
    Mindflayer made it, as the ledger notes (`created_files`), and nothing
    else is left in it. No block
    is left in a workspace's root, unless a project it links is there, nor
    in a folder above a project it links, whose agents would read it too,
    nor inside an agent's own folder (`AgentsMdError::UserScope`); a render
    that takes one out goes ahead there.
  - Claude's rule files, `.claude/rules/mindflayer/<route>.md`
    (`claude_rules`), hold a rule as written, front matter included. A
    render is handed what the ledger records of them
    (`claude_rules::Recorded`: each file's route and its `hash::bytes`),
    each file it may make recorded before it is made (`install::rules`), and
    a file is Mindflayer's only while it holds exactly that: only such a
    file is replaced or taken away. A file nothing
    recorded, even one holding the rule, or one changed since, is left
    exactly as it is and said, and so is a link at a rule's place. A new
    file is made with `create_new`; one replaced goes through
    `replace_bytes_unresolved`, whose temporary ends in `.tmp`, which
    Claude never loads; emptied folders go up to and including `mindflayer`
    and no further. A render asked for a rule is refused wherever the block
    is (`agents_md::misplaced`), and both renderers ask
    `RuleManifest::is_scoped` which rules are about some files only.
  - The import of `AGENTS.md` in a project's own `CLAUDE.md` or
    `.claude/CLAUDE.md` (`claude_md`) is one line between the block's
    markers, read by the same search (`agents_md::locate`), at the top of
    the file, an empty line below it that goes with it; a rule's marker
    there is refused like a broken block. Whether Claude reads the block is
    one predicate (`claude_md::reads_agents_md`): the three names in the
    project's root and every folder above it but `/`, the user memory file
    left out. `claude_md::reach` decides Native, the import, or Claude's
    rules folder (`Reach::mode`), never more than one, so Claude reads each
    rule once; an `@AGENTS.md` line somebody wrote in one of Claude's files
    counts as the import, and nothing is written. `claude_md::render` never
    makes a `CLAUDE.md` or a `CLAUDE.local.md`, never edits a file above
    the project (it is the fallback's reason instead), a link, the user
    memory file or the `AGENTS.md` itself, edits a file git tracks
    (`exclude::tracks_file`) in a local project only with `edit_tracked`,
    and takes the import out with the last rule, giving the file back as it
    was; one it may no longer take out is left and said.
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
  `install::targets` gives (each folder the project's agents read, or the one
  a `mind.toml` names), built from one tree and put in place together or not
  at all (`copy::Stage::place`). Standing is by the path each record names
  (`Ledger::copies`), never by name: anything unrecorded in any target is
  `Standing::Foreign`, refuses the whole install with nothing written in any
  target, and is never touched. A recorded copy is told by the hash of what
  was written there and by what its origin offers now (`install::Standing`,
  `install::standings`, D-04): `Current`, `Outdated`, `Edited`, `Missing` or
  `Orphaned`, the first true of edited, orphaned, missing and outdated said
  over the rest. An `Edited` copy refuses the whole install or uninstall as a
  foreign one does (`Installed::Edited`, `Removed::Edited`) unless it is
  forced (`install_with`, `uninstall_with` and `uninstall_typed` given
  `Edits::Forced`, the CLI's `--force`), and forcing never reaches a copy
  nothing records; a render keeps an edited section of the block as it holds
  it. A record with no hash counts as current only while its copy matches its
  offer byte for byte, and as edited otherwise; the first install over a match
  stores its hash. An update (`update::apply`) writes an `Outdated` or
  `Missing` installation again from the place its record says it came from,
  through the install's own body (`install::install_held`), and a project's
  rules in one render that keeps an orphaned rule as it is
  (`install::rules::update_held`); it leaves an `Edited` one, every copy of
  it, unless forced, and says an `Orphaned` one without failing. An
  uninstall removes the copies its record names and nothing else. No copy
  holds a skill's top-level `.claude-plugin`
  (`install::LeftOut`), and none lands inside an agent's own folder
  (`InstallError::UserScope`). A v3 project is listed from its agents' folders
  too, where an unrecorded copy outside the folder it authors in is
  `Provenance::Foreign` (by the path a record names, `Catalog::with_records`),
  and a lifecycle change looks only where the project authors
  (`Catalog::discover_authored`), so it never reaches one. Core's lifecycle
  changes see to the record of the copy the change reached, and only that
  copy's (D-04): `finish_edit` keeps it, so the copy reads as edited; `rename`
  carries it to the new name and place (`Ledger::renamed_copy`, dropping it
  only when another installation holds the new name); `remove` uninstalls the
  copy, dropping its record (`Ledger::uninstalled_copy`); and `add` drops a
  stale record under the name it writes. A v3 project whose authored folder is
  also a target keeps the record of its other copies, and a local one stops
  excluding a removed copy's place, or moves the entry with a renamed one.
  Each returns what it did (`lifecycle::Record`, and the `scrub` that opening
  the ledger to write ran; an edit only reads it), which the CLI only renders.
  A change of a project's agents or targets (`reapply::apply`) moves only what
  the ledger records: a new folder gets a copy of one whose `hash::tree` is
  the one recorded, never one over somebody's, and a dropped folder loses a
  recorded copy only while its hash is the one recorded; one changed since is
  left there, and its record and exclude entry dropped, since no install
  writes in that folder again. A project's move from a `mind.toml`
  (`migrate::project`) moves its installs the same way, through
  `reapply::retarget`, and holds a record from before hashes to its offer
  (D-04). Being made once, it is refused when the workspace `mind migrate`
  consults cannot be read, never made as if that one recorded nothing.
  Removing one of the workspace's own everywhere (`everywhere::apply`) and
  `uninstall --orphaned` (`assign::uninstall_orphaned`) take copies out only
  through `install::uninstall_with`, the first never forced, so an edited
  copy and somebody's are left as an uninstall leaves them.
  `install::disown` and `install::carry` are private to core, so no front end
  drops or moves a record on its own. The one way a copy nobody installed
  becomes recorded is `flayer adopt` (`adopt::apply`), asked for by name: it
  copies the skill into the library and records the project's copies, every
  target an install uses, under the `hash::tree` they hold, so they read as
  current (a target with no copy as missing, for an update to fill), never
  writes in the project, and refuses whatever an install could not have
  written there (a link, a top-level `.claude-plugin`, copies that differ,
  a skill `artifact::strict_issues` refuses), the project's own, and a name
  the library holds. A front end adds through
  `lifecycle::add`, never `create` or `create_from`, which leave a stale
  record in place. A rule is one installation too: in a project marked by its
  settings, its targets are its section of the block, recorded as `AGENTS.md`,
  and its file for Claude, each with a hash of its own
  (`Target::content_hash`, since the two hold different bytes); in one a
  `mind.toml` marks, its copy in the rules folder, filed by its route.
  Installing or uninstalling one renders every rule the project has
  (`install::rules`), each from the offer it was installed from: a section in
  the block, or a file at a rule's place for Claude, that the ledger does not
  record is `Standing::Foreign`, and a block holding such a section refuses
  the render (`NotRecorded`), as a rule no longer offered does
  (`NoLongerOffered`). What a project received as rules is listed from the
  ledger and the block (`status`, D-23), never by discovery, which reads no
  block, and a front end finds what to change through
  `lifecycle::find_to_change`, which refuses such a rule as received rather
  than as nothing.
- **Strict when writing, lenient when reading.** A skill's front matter is
  the Agent Skills format. `mind add`, `rename`, `install` and `flayer adopt`
  refuse a name or description that breaks its rules, all through
  `artifact::strict_issues`
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
  - So is a task's every value, which a browser will fill in: it reaches
    its command as that value, never as an option or a command. The task
    module spells each line's own words, and puts a value after the option
    that takes it (`=` when it starts with a dash), behind `./` for a `-C`
    or `-p`, or after a `--` among positionals; a test feeds every variant
    values that start with a dash.
  - A task or a query the panel sends names only what core's path policy
    allows (`policy`), which `admit::Admission` asks of every value that
    names something on the disk, in a match over every variant with no
    wildcard: a project or a loaded workspace by its entry as the marker
    spells it, which the backend turns into the absolute folder it resolves
    to for the line's one `-C` or `-p`, and a folder not linked yet by an
    absolute, normalized path outside every `.mindflayer`, verbatim. The
    browser supplies no `-C`, `--home`, `-W` or agent home: the backend runs
    every line with the launch's, against the workspace it was launched on,
    named. A refused task runs nothing, and every refusal has a code.
  - A file reaches the browser only as text inside JSON, no larger than
    `contents::CAP`, binary or larger reported by its size alone, and never
    served as itself (`contents::Contents`); its links are judged by
    `copy::followed`, the check `copy::tree` makes, so one leading out of its
    holder is refused: a project's root, or the folder the workspace keeps
    its own of that kind in, whose root may be a home. It is the only way: `show`, which prints an artifact
    whole through any link, is on the `Refusal` list (`show_unheld`).
  - An installed name must be one folder name for a skill
    (`install::is_folder_name`), and for a rule a route held to the name
    rules, which never climbs out (`install::is_installable_name`).
  - Symlinks are followed only inside the source (`copy::tree`'s `within`),
    and a rule offered for install that leads out of the workspace offering
    it is refused (`InstallError::Outside`).
  - A project's artifact directories must stay inside the project, and so
    must the folders its `targets` name, outside `.mind` and any
    `.mindflayer` too (`misplaced_target`): refused when a change names one,
    and passed over and said (`MindProject::misplaced_targets`) when the
    file was written by hand.
  - A config file is named by a route of folder names from the project, and
    a symlink anywhere on it, the file included, is refused, never followed
    (`config_entry::ConfigFile`): the route is looked at when the file is
    read and again just before it is written, and the write is never
    resolved. A folder made a link between that last look and the rename is
    the one window left (architecture.md, open questions).
  - An `AGENTS.md` that is a link leading out of its project is refused
    (`agents_md`), since a repository can link it to the user's own
    `~/.codex/AGENTS.md`; one to a file inside the project is written
    through. So is a rule file whose folders lead out of the project
    through a link (`claude_rules`), written or taken away, since `.claude`
    can be a link to the user's own `~/.claude`; the file itself is never
    followed. A `CLAUDE.md` reached through any link is never given the
    import (`claude_md`), whose path would then mean wherever the link
    leads, unless it is the project's `AGENTS.md` itself, which Claude then
    reads the block as (`Reach::Same`).
  - URLs are stored, logged and shown without credentials. `url::redacted`
    is the one function that takes them out: the ledger applies it to every
    URL it writes and reads, `GitError` to what it says, `git::scrub` to a
    clone's config and reflogs. A gather refuses a URL that carries one. A
    command that writes to the ledger opens it with
    `FlayerWorkspace::ledger_to_write`, which scrubs what an older build
    stored first and hands back what it took out to be said; one that only
    reads opens it with `ledger_for_reading`, which scrubs nothing. The
    install screen is a read until it applies, and the lines it then runs
    scrub as any `install` does. Anything new that keeps or prints a URL goes
    through `url::redacted` too, a serialized one included
    (`url::serialize_redacted`), whatever built the value.
- **Writers take turns; readers never wait.** Core's writers hold the
  workspace's `.mindflayer/lock` (`FlayerWorkspace::lock`) while they change
  it: `edit_array`, `install::install` and `uninstall`,
  `install::rules::render`, `gather::gather`,
  `gather::scrub`, which `FlayerWorkspace::ledger_to_write` runs, when it has
  clones to rewrite, `migrate::library`, from its plan to its rewrite of
  the marker, `migrate::project`, for a move from a `mind.toml`, from before
  it reads the project again until its ledger is written (a move from a
  `settings.json` moves no copy, and takes none), `reapply::apply`,
  from before it reads a
  project's settings until their change and every copy it carries along are
  written, `relink::relink`, from its ledger checks until the entry and the
  records it moves are written, `relink::new_id`, `adopt::apply`, from the
  plan it makes again until its record is written, and `update::apply`, from the plan it makes again until its last
  installation is written, through `install::install_held` (the body
  `install_with` hands to once it holds the lock) and
  `install::rules::update_held`, which take none of their own. A new
  writer takes it the same way, inside core; `config_entry::ConfigFile`,
  `agents_md::render`, `claude_rules::render`, `claude_md::render` and
  `MindProject::change_settings` take none, since
  the writers that call them hold it, and `assign::install` and
  `uninstall` none across their batch, leaving it to each project's
  install, in the install screen's lines too. A second workspace linking
  the same project is kept out by neither lock, an open question in
  `docs/architecture.md`.
  Opening the ledger takes no lock: a schema migration keeps itself apart
  in its own transaction. The lock is not re-entrant: a writer never calls
  another while it holds it, and a front end never takes it. Reads
  (`FlayerWorkspace::open`, `ledger_for_reading`) never take it. A wait is
  bounded (`lock::WAIT`), ending in a `LockError::Busy` that names the
  holder's pid, except a record drop's or carry's (`install::disown`,
  `install::carry`), which follows a change already made.
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
  writes, for tests of such a project; `settings.json` at `SETTINGS_VERSION`,
  or `TARGETS_VERSION` when it names `targets`, which an older build would
  read past, read as written and changed by nothing but `mind migrate`; and
  `settings.toml` at `SETTINGS_TOML_VERSION` whatever it sets, which `mind
  init` writes (`MindProject::init_asked`, which fills in each folder it was
  not told about). Each version is written in one file
  (`MarkerFile::of_version`), and one found in another is refused. Any of the
  three makes a directory a project (`MindProject::is_marked`), and the
  newest wins when more than one is there. See `DIRECTORIES_VERSION` for how
  a v1 `mind.toml` is still read, and `SETTINGS_VERSION` for why a kind a
  settings file does not name has no folder (`directory_for` is `None`).
  Anything from a newer version is refused. A format change must keep
  reading old files the way they were written, and move one forward only
  when asked, never on a read (`flayer migrate --library`, `mind migrate`).
  A key an older build reads past without misreading anything moves no
  version, as the default workspace's `known` did not.
  - A ledger schema step is one more block in `Ledger::migrate`, which takes
    the write lock, copies the file to `mindflayer.db.v<old>.bak`
    (`Ledger::back_up`) and runs every step in one transaction. A step fills
    in old rows in its own SQL, not through today's code, and adds a fixture
    of what the version before it wrote
    (`apps/core/tests/fixtures/ledger-v<N>.sql`) for a test to migrate.
    What no SQL can know is handed to the step rather than read by it:
    schema 5 is told each linked project's id (`Ledger::open_knowing`, from
    `FlayerWorkspace::known`).
- **A project is found again by its id, never by a guess.** The ledger
  keeps the id it last saw at each route (`projects`), learned from the
  projects themselves every time it is opened to write (`Ledger::knew`),
  and a stale entry keeps its id until `relink::relink` moves it. A relink
  hands records only to a directory holding that id, from an entry nothing
  marks any more (`MindProject::is_vacant`: one whose marker would not open
  still holds its project), and two directories holding one id are a copy
  (`relink::Duplicate`), a collected failure and never a move.
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
  copy inside one, as `agents_md::render` and `claude_rules::render` take
  them to refuse rules there, and `claude_md` to leave the user memory file
  out of what keeps Claude from reading `AGENTS.md`. `tui::execute` hands each line the TUI's own
  with the rest of its `Lookup`, and so does its install screen: a command
  the TUI runs carries none of the four options, so it would decide them
  again from the environment.
- **The server holds no rules.** `apps/server` carries JSON between the
  browser and a `Backend`, whose `Query`, `Task` and `Outcome` are associated
  types, and depends on no mindflayer crate (`tests/crate_test.rs` checks it
  with `cargo metadata`); a front end depends on it, as the CLI does
  (`backend.rs`), never the other way. A check, path rule or naming rule
  written there is one the panel could disagree with the CLI about: it
  belongs in core, reached through the backend, which says it as a
  `Refusal`, a code and a message, sent as a 422; a task refused when its
  lines are asked for is never run, nor kept in the transcript. Queries and
  previews run on tokio's blocking pool, and every run on the one writer
  thread, in the order it arrived. It binds
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
  runs nothing. Nor does it remember the workspace in the default one's
  `known`, though #56 asked it to: `flayer init` does (`known::remember`),
  only once the default is made, and never makes it (architecture.md, "The
  workspaces this machine knows", and its open questions). Its shell comes
  from `--shell`, then `$SHELL`, read by the front end. The code undoes only
  what it did, through `flayer-deactivate` and never `deactivate`, and a
  workspace's name reaches a prompt only through `activate::prompt`.
- **`Kind` and `Agent` are closed enums.** To add a kind, follow "Adding a
  third kind" in the architecture doc, and to add an agent, "Adding an agent",
  which starts in `docs/agents.md`. Either way the compiler's exhaustive
  matches are the checklist: the target table's `standard`, and `read` behind
  `Agent::reads`, match every kind and agent with no wildcard, so neither
  compiles until it says where it goes and where it reads; `at_root`, behind
  `Agent::reads_at_root`, matches every agent the same way.
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
  authors in, makes one with `common::older_project`, and one about a
  project a `settings.json` marks with `common::json_project`, which writes
  the file as a version 3 `mind init` did. What git would list is
  asked of gix too (`common::untracked`), and a core fixture has git track
  its files by writing the index with gix (`common::indexed`). A core test
  about which `CLAUDE.md` Claude finds first holds its temporary folder to
  have none above it (`claude_md_test.rs`'s `clean`).
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
  (`common::request`, then `common::deliver`). The CLI's backend has a suite
  of its own (`apps/cli/tests/backend_test.rs`), which drives `PanelBackend`
  through the `Backend` trait with the JSON a browser sends, binding no
  port.
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
