# Developing Mindflayer

How the repository is laid out, how to build and test it, and how to work on
the panel. The [README](../README.md) is where using Mindflayer starts, and
[the reference](reference.md) has every command; [architecture.md](architecture.md)
says why each part is the way it is.

## Repository layout

Every unit of the product is an app under `apps/`, so adding a front end means
adding a directory rather than reshaping the tree:

```
apps/core    mindflayer-core     projects, workspaces, artifacts.
                                 No I/O beyond files, and git when gathering.
apps/cli     mindflayer-cli      the `mind` and `flayer` binaries, the TUI,
                                 and the panel's backend. Parsing and
                                 rendering only; all of it goes through one
                                 parser.
apps/server  mindflayer-server   the local server behind the web panel, which
                                 `flayer ui` runs. Transport only: it moves
                                 JSON between the browser and a front end,
                                 and holds no rules.
apps/web     mindflayer-web      the web panel the server embeds: Vue and
                                 Nuxt UI, built with Vite. It asks through
                                 the server and draws; no rules here either.
```

See [architecture.md](architecture.md) for why the split is where it
is. `examples/` sits beside `apps/` and is not an app: it is the workspace
[to try Mindflayer with](reference.md#try-it-with-the-examples).

## Development

A root `Makefile` delegates to each app, and CI invokes the same targets.
`make help` lists the root ones; each app's own, such as `cli/release`, are in
`apps/<app>/Makefile`:

```bash
make test              # every suite, the panel's first: needs Node 24 and pnpm
make build             # the panel, then a debug build of every crate
make fmt               # cargo fmt --all
make lint              # no em dash in a tracked file, then clippy
make run ARGS="list"   # run the CLI without installing it

make core/test         # one app: make <app>/<target>
make cli/run ARGS="flayer list"
```

Tests live in their own files, named for what they test and ending in
`_test.rs`: `apps/<app>/tests/<area>_test.rs` for each area of the command
surface, with what they share in `tests/common/`, and a module's unit tests in
`src/<module>_test.rs` beside it. No test touches your home, or a workspace
your shell has activated: the CLI suites run every command with `--home ""`
and `--workspace ""`, and the few that run the binaries clear
`$MINDFLAYER_HOME` and `$MINDFLAYER_WORKSPACE` and give them a home of their
own. Nor do they touch your agents' folders, which no command reads yet, and
the commands they run themselves give every agent-home option empty. The server's suites never leave 127.0.0.1, and run
against a fake front end rather than the CLI. The CLI's `ui` suite starts
`flayer ui` on 127.0.0.1 with `--no-open`, so no test opens a browser.
`docs_test.rs` holds the README and every page here to their relative
links: a file one names has to be there, an anchor has to be a heading's,
and every image in `docs/images` has to be shown by some page.

```bash
cargo test -p mindflayer-cli --test tui_test          # one area
cargo test -p mindflayer-core --lib workspace         # one module's unit tests
```

Working on it, the loop is `make dev/link` once, then:

```bash
make dev/watch         # rebuilds on every change; the symlink stays current
```

`make dev/watch` needs `cargo-watch` (`cargo install cargo-watch`) and says so
if it is missing. Watch out for the version of that message which is not about
being missing at all: `cargo install` puts it in `~/.cargo/bin`, so if that is
not on your PATH, `make dev/watch` reports it as uninstalled while it sits
there installed. Note also that `make clean` deletes `target/`, which leaves
the `dev/link` symlinks dangling until the next build.

`BINDIR` says where the symlinks go, if `~/.cargo/bin` is not where you want
them:

```bash
make dev/link BINDIR=~/.local/bin
make dev/link BINDIR=.          # `./mind`, `./flayer` and `./mf`, right here
```

The repository root is a fine answer if you would rather not put a
work-in-progress binary on your PATH at all: `.gitignore` already covers all
three names.

`scripts/agent-probe/probe.sh` checks on the real agents what
[agents.md](agents.md) records. It runs them against their APIs with
your own login, in a temp home under `target/` with your real home hidden, so
neither CI nor `make test` runs it. It needs Linux, `bwrap`, `jq` and `git`
in `/usr/bin` or `/bin`, and Claude Code's native binary rather than an npm
launcher or a version manager's shim.

## Developing the panel

`apps/web` is the web panel, in Vue 3.5 and Nuxt UI 4 on Vite, written in
TypeScript: the overview, the projects, the library, the shelf and the
sources, in a frame of a sidebar and a header, dark unless you switch it to
light, which the next launch remembers, with a link and an unlink run
through the server's previews and a transcript of what ran. It needs Node 24 (`apps/web/.node-version`) and pnpm, at
the version `package.json` pins in `packageManager`, which `corepack enable`
provides. Without corepack, name any other way to run it in `PNPM`:

```bash
make web/test          # vitest
make web/lint          # ESLint, its style included; make web/fmt fixes that
make web/build         # check the types, then build apps/web/dist
make web/types         # write the types from Rust, then check the panel's (vue-tsc)
make web/types/check   # fail when the types from Rust are not the ones committed
make web/typecheck     # check the panel's types alone (vue-tsc)
make web/clean         # remove dist/, what the browser suite wrote, and the tools' caches

make web/e2e/browsers  # download Chromium and WebKit for the browser suite, once
make web/e2e           # build the panel and the binaries, then drive them in both
make docs/screenshots  # build them, then take the README's screenshots in Chromium

make web/test PNPM='npx --yes pnpm@12.15.0'
```

Every target that runs a tool first installs what `pnpm-lock.yaml` pins,
when `node_modules` is missing or older than `package.json`, the lockfile or
`pnpm-workspace.yaml`. `make test` and `make build` run the panel's targets
first, so they need Node 24 and pnpm too; `make core/test`, `make cli/test`
and `make server/test` do not, nor does `make cli/build`, since the server
builds without the panel. Its tests are `apps/web/tests/<area>.test.ts`.

What the panel reads and sends has its types written from the Rust that
serializes it, into `apps/web/src/api/generated`, which is committed and
never edited by hand. After changing a type core or the CLI hands the panel,
`make web/types` writes them again and checks the panel against them; `make
web/types/check` needs only cargo and git, and fails when what is committed
is not what Rust writes, as CI's `types drift (web)` check does.

`make web/build` writes `apps/web/dist`, which the server embeds as it
compiles, and marks the server to compile again, so the next build of the
binaries takes the panel in. One built before the panel answers every page
with one saying to run it. `make install` and `make cli/release` build the
panel first where Node and pnpm run; without them they warn, and embed what
`apps/web/dist` already holds, if anything. `make cli/release/check
TARGET=<triple>` starts the release built for that triple, once
`apps/web/dist` is gone, and fails unless it serves the panel from the binary
itself, as CI checks on every run.

`make web/e2e` tests the whole chain in a browser: it builds the panel and
the debug binaries, builds a workspace in your system's temporary folder by
running `flayer` and `mind`, serves it with `flayer ui`, and drives the
panel in Chromium and WebKit with Playwright. The overview, each project's
page and each artifact's page have to show what `flayer list --json`
prints, a project linked or unlinked from the panel has to leave
`flayer.toml` byte for byte as `flayer link` or `flayer unlink` leaves a copy
of the workspace, an unlink has to ask first and run nothing when told no, a
request from another origin has to be refused, and no page may log a
violation of the server's content security policy. A test that changes
something does it to a workspace and a `flayer ui` of its own, so no other
test sees the change. Nothing it runs can
reach your workspaces or your agents' folders: Playwright runs without
`$MINDFLAYER_WORKSPACE`, `$MINDFLAYER_HOME`, `$CLAUDE_CONFIG_DIR`,
`$CODEX_HOME` and `$OPENCODE_CONFIG_DIR`, the server and every command get
an empty temporary folder as `HOME`, and every command names its workspace
and gives every agent-home option empty. The suite fails if that home is not
empty once the server has stopped. Its specs are
`apps/web/e2e/<area>.spec.ts`, and `make test` leaves them out, since they
need the browsers and binaries built with the panel. `ARGS` go to
`playwright test`, and `E2E_BIN` names another folder holding a `flayer` and
a `mind` to drive, relative to `apps/web`:

```bash
make web/e2e ARGS='e2e/csp.spec.ts --debug'
make web/e2e/run E2E_BIN=../../target/x86_64-unknown-linux-gnu/release
```

The browsers are Playwright's own builds, which `make web/e2e/browsers`
downloads into `apps/web/node_modules` rather than `~/.cache`. Its WebKit
needs libraries Playwright installs only on Debian and Ubuntu, with sudo
(`make web/e2e/browsers WITH_DEPS=1`), so anywhere else, Arch among them,
run the suite in Chromium alone with `make web/e2e BROWSERS=chromium`, and
leave WebKit to CI's `e2e (web)`, which drives the release `build (cli)`
built.

`make web/dev` serves the panel on `http://127.0.0.1:5173` and reloads it as
you edit. It passes `/api`, and the launch link, on to `flayer ui --dev`,
run beside it, which listens on port 7878, where the dev server's proxy
looks, takes requests from the dev server's pages, and prints and opens a
link to the dev server: the token is traded for its cookie where the panel
runs.

Three rules keep the panel working under the server's content security
policy, which lets nothing in from another host and nothing inline:

- **The transport is the only way out.** Views ask `useTransport()` and
  never `fetch`: ESLint refuses `fetch`, `EventSource`, `WebSocket` and
  `XMLHttpRequest` everywhere but `src/api/http.ts`.
- **Nothing comes from another host.** The fonts are Geist and Geist Mono
  from Fontsource, the icons Lucide's, bundled when a component names them
  as `i-lucide-<name>`. A test builds the panel and fails on any URL that
  names another host.
- **Nothing inline.** No `<script>` or `<style>` written into the page, which
  is why the palette is declared in `src/assets/css/main.css` (from the
  mapping in `src/theme.ts`) rather than injected, as Nuxt UI would. A
  skill's markdown is drawn element by element from its tokens
  (`src/markdown.ts`), never put on the page as HTML.

A test also checks that every package installed carries a licence
GPL-3.0-only can take in.

### The README's screenshots

The README tours the panel in pictures, and none is taken by hand. `make
docs/screenshots` builds the panel and the debug binaries, as `make web/e2e`
does, then runs `apps/web/e2e/screenshots.ts`: it builds a demo workspace by
running `git`, `flayer` and `mind` (`e2e/demo.ts`: six projects, a library,
a shelf gathered from a repository beside them, three installs behind
where they came from, two repositories nobody linked), serves it with
`flayer ui`, and opens each page of the tour in Chromium, in a 1440 by 900
viewport at a device scale of 1.25, writing each into `docs/images`. It
runs as the browser suite does, with no variable that leads to your
workspaces or your agents' folders, an empty temporary home that has to be
empty still at the end, and every command naming its home and every agent
home. The project's page is taken in dark as well, for the README's
`<picture>`; the rest of the tour is light.

The demo is built in `mindflayer-demo` under the system's temporary folder,
`/tmp/mindflayer-demo` on Linux, always the same place, since every page
shows its paths. A run refuses that folder when something other than an earlier
run of its own is there, and removes it when it is done. Before it writes
an image, it reads the page for a path under a home (other than
`/home/you`, the reference's stand-in, which the panel's own placeholders
use), for your account's name as a folder, and for the launch token, and
fails on any of them; it fails too on an image over 300 KB, which
regenerating would add to the history every time.

Run it whenever what the panel shows changes, look at every image it
wrote, and commit them with the change. Chromium is enough, `make
web/e2e/browsers BROWSERS=chromium`; like `make web/e2e` it leaves
`apps/web/dist` behind, which `make web/clean` removes.
