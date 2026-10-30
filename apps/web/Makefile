.DEFAULT_GOAL := build
.PHONY: install build build/if-node test lint fmt types types/export types/check fixtures fixtures/check typecheck dev e2e e2e/run e2e/browsers screenshots screenshots/run clean

# pnpm at the version package.json's packageManager pins. Where it is not on
# the PATH and corepack is not set up, name it some other way:
#   make web/test PNPM='npx --yes pnpm@12.15.0'
PNPM ?= pnpm

# pnpm writes this as it installs, and every install here touches it after,
# so it is newer than the manifest, the lockfile and pnpm's settings only
# when node_modules holds what they say. Every target that runs a tool waits
# on it, which installs on a fresh clone and after a change to any of them,
# and costs nothing otherwise.
INSTALLED := node_modules/.modules.yaml

$(INSTALLED): package.json pnpm-lock.yaml pnpm-workspace.yaml
	$(PNPM) install --frozen-lockfile
	@touch $@

install: ## Install exactly what the lockfile pins
	$(PNPM) install --frozen-lockfile
	@touch $(INSTALLED)

# The server embeds dist/ as it compiles, and one compiled while dist/ was
# missing embeds nothing and never looks again, since rust-embed resolves the
# folder once. Touching the file that embeds it is a change cargo sees, so
# the server's next compile takes the panel in. A build script watching the
# folder was the alternative, and it would recompile the server on every
# build while the folder is missing, as it is wherever Node is not.
build: $(INSTALLED) ## Check the types, then build the panel into dist/, which the server embeds
	$(PNPM) run build
	@touch ../server/src/panel.rs

# What a binary about to be built embeds, for `make install` and `make
# cli/release`: the panel built now, where Node and pnpm run, and otherwise
# what dist/ already holds, said in a warning, since the binary then ships a
# panel built some other time, or only the page saying none was. `build`
# alone made both fail without Node, though a binary without the panel
# still has every command. Node and pnpm both: pnpm runs the tools on the
# `node` the PATH gives, and CI's runners have a Node and no pnpm. Only a
# missing tool is passed over: a build that fails fails this too. With no
# build, the file that embeds dist/ is touched as `build` touches it, so a
# server compiled while the folder was missing, then filled from elsewhere,
# takes it in.
build/if-node: ## Build the panel where Node and pnpm run, or warn what a binary built now embeds
	@missing=; \
	if ! command -v node >/dev/null 2>&1; then \
		missing="Node is not on the PATH"; \
	elif ! $(PNPM) --version >/dev/null 2>&1; then \
		missing="pnpm does not run as \`$(PNPM)\`"; \
	fi; \
	if [ -z "$$missing" ]; then \
		$(MAKE) build; \
	elif [ -f dist/index.html ]; then \
		echo "warning: $$missing, so the panel is not built: the binaries embed the one apps/web/dist holds, from when it was built" >&2; \
		touch ../server/src/panel.rs; \
	else \
		echo "warning: $$missing, so the panel is not built: the binaries' \`flayer ui\` serves only a page saying so" >&2; \
	fi

test: $(INSTALLED) ## Run the panel's tests (vitest)
	$(PNPM) run test

lint: $(INSTALLED) ## Lint the panel, its style included (ESLint)
	$(PNPM) run lint

fmt: $(INSTALLED) ## Fix what the linter can fix, its style above all
	$(PNPM) run fmt

# What the panel takes from Rust: the types core's and the CLI's `ts`
# feature write here (TS_RS_EXPORT_DIR, in .cargo/config.toml), committed so
# a fresh clone type-checks, and never edited by hand.
GENERATED := src/api/generated

# Cargo alone, so it runs where Node does not, CI's `types drift (web)`
# among them. Emptied first, since ts-rs writes and never deletes: a type Rust no
# longer hands over would leave its file behind. The export tests and the
# ones holding each type to what serde writes are all named for it, and run
# alone. The CLI's export writes core's types it names again, as core's
# writes them, so which crate's runs first changes nothing.
types/export: ## Write the types the panel takes from Rust into src/api/generated (cargo only)
	rm -rf $(GENERATED)
	cargo test -p mindflayer-core -p mindflayer-cli --features mindflayer-core/ts,mindflayer-cli/ts --lib typescript

types: types/export $(INSTALLED) ## Write the types from Rust, then check the panel's against them (vue-tsc)
	$(PNPM) run typecheck

# `git diff --exit-code` for a file that changed or went, and an untracked
# one for a type that is new: diff says nothing of a file git has not seen.
types/check: types/export ## Fail if the types written from Rust are not the ones committed (cargo and git only)
	@git diff --exit-code --stat -- $(GENERATED) \
		&& test -z "$$(git ls-files --others --exclude-standard -- $(GENERATED))" \
		|| { git ls-files --others --exclude-standard -- $(GENERATED); \
			echo "error: the types Rust writes are not the ones committed: run make web/types and commit apps/web/$(GENERATED)" >&2; \
			exit 1; }

# The snapshots the panel's tests render from, and what opening each of the
# library's own is answered with, written by a test of the CLI's
# (tests/fixtures_test.rs) from workspaces it builds by running the
# commands, read through the panel's backend: committed, so the tests run
# without cargo, and written again by cargo alone, as the types are. Emptied
# first, for the reason the types are. Under the `ts` feature, so a plain
# `cargo test` writes none of them.
FIXTURES := tests/fixtures

fixtures: ## Write the snapshots and answers the panel's tests render from into tests/fixtures (cargo only)
	rm -rf $(FIXTURES)
	cargo test -p mindflayer-cli --features ts --test fixtures_test

# As types/check: a fixture that changed or went, or one new to git.
fixtures/check: fixtures ## Fail if the snapshots written from Rust are not the ones committed (cargo and git only)
	@git diff --exit-code --stat -- $(FIXTURES) \
		&& test -z "$$(git ls-files --others --exclude-standard -- $(FIXTURES))" \
		|| { git ls-files --others --exclude-standard -- $(FIXTURES); \
			echo "error: the snapshots Rust writes are not the ones committed: run make web/fixtures and commit apps/web/$(FIXTURES)" >&2; \
			exit 1; }

typecheck: $(INSTALLED) ## Check the panel's types alone (vue-tsc)
	$(PNPM) run typecheck

dev: $(INSTALLED) ## Serve the panel on 127.0.0.1:5173, /api proxied to `flayer ui --dev --port 7878`
	$(PNPM) run dev

# The end-to-end suite (e2e/): the panel served by the real `flayer ui`
# over a workspace built with the real commands, driven in a browser, with
# Playwright. Its driver, e2e/run.ts, builds that workspace and its home in
# the system's temporary folder, clears every variable that leads to a real
# workspace or agent home, runs Playwright, and fails if the home is not as
# it was made once the server has stopped.
#
# The binaries it runs: the debug build `e2e` makes, or any folder holding a
# `flayer` and a `mind` with the panel in them, as CI names the release
# `build (cli)` built. A path relative to this folder, or absolute.
E2E_BIN ?= ../../target/debug

# The browsers it runs in: Chromium, and WebKit, the engine a desktop shell
# would draw the panel with. Playwright's WebKit needs libraries it installs
# only on Debian and Ubuntu (`WITH_DEPS=1`, with sudo, as CI does), so on
# any other system, Arch among them: make web/e2e BROWSERS=chromium
BROWSERS ?= chromium webkit

# Where Playwright keeps the browsers it downloads: inside node_modules,
# which git ignores and `pnpm install` leaves alone, rather than its default
# under ~/.cache, so what the suite installs stays with the clone. Exported,
# so Playwright looks there wherever a target here runs it.
export PLAYWRIGHT_BROWSERS_PATH ?= $(CURDIR)/node_modules/.playwright

e2e/browsers: $(INSTALLED) ## Download the browsers the end-to-end suite runs in (BROWSERS), into node_modules
	$(PNPM) exec playwright install $(if $(WITH_DEPS),--with-deps) $(BROWSERS)

# The panel, then the binaries, which a server compiled before dist/ was
# there would leave without it (see `build`), then the suite on them.
e2e: build ## Build the panel and the debug binaries, then run the end-to-end suite on them
	cargo build -p mindflayer-cli
	$(MAKE) e2e/run

# ARGS go to `playwright test`: a spec, `-g <title>`, `--debug`, `--ui`.
e2e/run: $(INSTALLED) ## Run the end-to-end suite on the flayer and mind in E2E_BIN, in BROWSERS
	MINDFLAYER_E2E_BIN='$(E2E_BIN)' node e2e/run.ts $(addprefix --project=,$(BROWSERS)) $(ARGS)

# The README's screenshots, in ../../docs/images: the demo workspace
# e2e/demo.ts builds with the commands, served by the binaries' `flayer ui`
# and taken in Chromium by e2e/screenshots.ts, which fails on a page showing
# a home's path, the account's name or the launch token, and on an image
# over its budget. Chromium alone, which `make e2e/browsers
# BROWSERS=chromium` downloads: the images are the same in any engine, and
# WebKit does not run everywhere. Neither part of `test` nor of CI: the
# images are taken when what the panel shows changes, and committed.
screenshots: build ## Build the panel and the debug binaries, then take the README's screenshots into docs/images
	cargo build -p mindflayer-cli
	$(MAKE) screenshots/run

screenshots/run: $(INSTALLED) ## Take the README's screenshots with the flayer and mind in E2E_BIN
	MINDFLAYER_E2E_BIN='$(E2E_BIN)' node e2e/screenshots.ts

clean: ## Remove the built panel, what the end-to-end suite wrote and the tools' caches, keeping node_modules
	rm -rf dist test-results playwright-report node_modules/.cache node_modules/.vite node_modules/.tmp
