.DEFAULT_GOAL := help

# Every unit of the product lives under apps/. Adding one (a desktop shell)
# means a directory with its own Makefile and a line here.
#
# The panel first, so `make build` compiles the crates after it: the server
# takes it in as it compiles, and a panel built last would leave the
# binaries of that same run without it, the server marked to compile again
# only on the next. It also puts the one app that needs Node and pnpm first
# in `make test`, so a machine without them is told at once rather than
# after every Rust suite.
APPS := web core cli server

# ---------------------------------------------------------------------------
# Per-app delegation: `make <app>/<target>` runs <target> in apps/<app>/Makefile
# e.g. `make core/test`, `make cli/run ARGS="list"`, `make web/build`
# ---------------------------------------------------------------------------
define APP_DELEGATE
$(1)/%:
	$$(MAKE) -C apps/$(1) $$*
endef
$(foreach app,$(APPS),$(eval $(call APP_DELEGATE,$(app))))

# ---------------------------------------------------------------------------
# Global targets: CI invokes these, so they are the contract, not cargo.
#
# The aggregates are derived from APPS rather than spelled out, so that adding
# an app really is the one line above: a name left out of `test` would be an
# app CI silently never runs.
# ---------------------------------------------------------------------------
.PHONY: test
test: $(addsuffix /test,$(APPS)) ## Run every test suite, the panel's first (Node 24 and pnpm)

.PHONY: build
build: $(addsuffix /build,$(APPS)) ## Build the panel, then every crate (debug)

.PHONY: fmt
fmt: ## Format all Rust code
	cargo fmt --all

.PHONY: fmt/check
fmt/check: ## Fail if any Rust code is not formatted
	cargo fmt --all --check

# The features that describe what the crates hand a front end: the panel's
# TypeScript (`ts`, core's and the CLI's) and the API's JSON Schema
# (`schema`, the server's too).
TS_FEATURES := mindflayer-core/ts,mindflayer-cli/ts
SCHEMA_FEATURES := mindflayer-core/schema,mindflayer-server/schema,mindflayer-cli/schema

# Twice: as everything builds, and with the `ts` and `schema` features,
# whose code (the panel's types, the API's schemas and the tests holding
# both to serde) nothing else lints. Its own run rather than the one with
# the features on, which would leave the build without them, the one every
# binary ships as, unlinted; the two together, as `make api/schemas` builds
# them, rather than a run for each.
.PHONY: lint
lint: lint/em-dash ## Run clippy across the workspace, then with the ts and schema features, once no em dash is found
	cargo clippy --workspace --all-targets
	cargo clippy --workspace --all-targets --features $(TS_FEATURES),$(SCHEMA_FEATURES)

# The API's JSON Schemas (#217, D-24): every type core, the CLI and the
# server hand the API whole or take from it, written from Rust by the
# export tests the `schema` feature compiles, a file a root, into
# docs/api/schemas, committed for the API's description (#218) and any
# client to read, and never edited by hand. "api" is no app, so these do
# not collide with the <app>/<target> rules above. Emptied first, since an
# export writes and never deletes: a root Rust no longer has would leave
# its file behind. The export tests and those holding each schema to what
# serde writes are all named for it, and run alone; with `ts` too, so the
# test holding every root of the panel's types to a schema runs beside
# them. Cargo alone, as the panel's types are, so CI's `types drift (web)`
# runs it.
SCHEMAS := docs/api/schemas

.PHONY: api/schemas
api/schemas: ## Write the API's JSON Schemas from Rust into docs/api/schemas (cargo only)
	rm -rf $(SCHEMAS)
	cargo test -p mindflayer-core -p mindflayer-server -p mindflayer-cli --features $(SCHEMA_FEATURES),$(TS_FEATURES) --lib schema

# As web/types/check: `git diff --exit-code` for a file that changed or
# went, and an untracked one for a root that is new.
.PHONY: api/schemas/check
api/schemas/check: api/schemas ## Fail if the schemas written from Rust are not the ones committed (cargo and git only)
	@git diff --exit-code --stat -- $(SCHEMAS) \
		&& test -z "$$(git ls-files --others --exclude-standard -- $(SCHEMAS))" \
		|| { git ls-files --others --exclude-standard -- $(SCHEMAS); \
			echo "error: the schemas Rust writes are not the ones committed: run make api/schemas and commit $(SCHEMAS)" >&2; \
			exit 1; }

# The API's OpenAPI 3.1 document (#218, D-24): every route of the server's
# own table and of the CLI's backend's, with the schemas above as its
# definitions, written from both tables by the CLI's export test, which the
# `schema` feature compiles, into docs/api/openapi.json, committed and never
# edited by hand. One file, written over, so nothing is emptied first. With
# the features `api/schemas` builds with, so the two share one build, and
# cargo alone, so CI's `types drift (web)` runs it beside them.
OPENAPI := docs/api/openapi.json

.PHONY: api/openapi
api/openapi: ## Write the API's OpenAPI document from Rust into docs/api/openapi.json (cargo only)
	cargo test -p mindflayer-cli --features $(SCHEMA_FEATURES),$(TS_FEATURES) --lib openapi

# As api/schemas/check, for the one file.
.PHONY: api/openapi/check
api/openapi/check: api/openapi ## Fail if the OpenAPI document written from Rust is not the one committed (cargo and git only)
	@git diff --exit-code --stat -- $(OPENAPI) \
		&& test -z "$$(git ls-files --others --exclude-standard -- $(OPENAPI))" \
		|| { echo "error: the OpenAPI document Rust writes is not the one committed: run make api/openapi and commit $(OPENAPI)" >&2; \
			exit 1; }

# The em dash reads as machine-written, so `lint` keeps it out of every
# tracked file: docs, comments, messages and test expectations alike, the
# panel's included, which is why this is not a clippy lint. Tracked files
# only, because an untracked folder (an agent's worktree, a scratch clone)
# is not the repository's text. The character is spelled as its UTF-8 bytes
# so this file does not hold one. `git grep` exits 0 on a match and 1 on
# none; anything else is git failing, which must not pass for a clean tree.
.PHONY: lint/em-dash
lint/em-dash: ## Fail if a tracked file holds an em dash
	@git grep -n -I -F "$$(printf '\342\200\224')"; status=$$?; \
	if [ $$status -eq 0 ]; then \
		echo "error: an em dash above; write a comma, a colon, parentheses or a new sentence instead" >&2; \
		exit 1; \
	elif [ $$status -ne 1 ]; then \
		exit $$status; \
	fi

# The README's screenshots are taken from the panel, never by hand: the
# panel and the binaries built, then every page of its tour taken over a
# demo workspace (apps/web/e2e/screenshots.ts). "docs" is no app, so this
# does not collide with the <app>/<target> rules above. It needs Node, pnpm
# and Playwright's Chromium (`make web/e2e/browsers BROWSERS=chromium`).
.PHONY: docs/screenshots
docs/screenshots: ## Build the panel and the binaries, then take the README's screenshots into docs/images
	$(MAKE) -C apps/web screenshots

.PHONY: run
run: ## Run the CLI (pass args via ARGS="list")
	$(MAKE) -C apps/cli run ARGS="$(ARGS)"

.PHONY: clean
clean: ## Remove build artifacts
	cargo clean

# ---------------------------------------------------------------------------
# Getting the binaries onto your PATH
#
# Two ways, for two situations. `install` is a copy: it is what you want if you
# just want to use the tools. `dev/link` is a symlink to the debug build, so
# every build of the CLI updates the binaries on your PATH without
# reinstalling anything, which is what you want while working on them:
# `make cli/build` with Rust alone, since `make build` builds the panel first
# and needs Node and pnpm.
#
# Both binaries, not just `mind`: `flayer` is a second entry point into the
# same crate, and half an install is worse than none.
#
# `mf` is a short name for `flayer` (the one that manages minds, so `mf init`
# makes a workspace), linked beside the two: a symlink rather than a third
# binary, so it cannot be a different version of the same one. Only a link,
# never a cargo `[[bin]]`: `mf` is METAFONT wherever TeX is installed, and a
# link is something you asked for on your own machine.
#
# "dev" is not in APPS, so these do not collide with the <app>/<target> pattern
# rules above. BINDIR governs the symlinks; `install` goes wherever cargo puts
# its binaries.
# ---------------------------------------------------------------------------
BINDIR ?= $(HOME)/.cargo/bin
BINS := mind flayer
# alias:binary pairs, linked by dev/link and removed by dev/unlink.
ALIASES := mf:flayer

# The panel first, as for a release, where Node and pnpm run: the copies
# embed it as they compile. Elsewhere a warning says what they embed instead
# (`make web/build/if-node`).
.PHONY: install
install: ## Install `mind` and `flayer` as copies in cargo's bin directory, the panel built first where Node is
	$(MAKE) -C apps/web build/if-node
	cargo install --path apps/cli --locked

.PHONY: dev/link
dev/link: ## Build, then symlink `mind`, `flayer` and `mf` into BINDIR
	cargo build -p mindflayer-cli
	@mkdir -p "$(BINDIR)"
	@for bin in $(BINS); do \
		ln -sfn "$(CURDIR)/target/debug/$$bin" "$(BINDIR)/$$bin"; \
		echo "$(BINDIR)/$$bin -> $(CURDIR)/target/debug/$$bin"; \
	done
	@for pair in $(ALIASES); do \
		alias=$${pair%%:*}; bin=$${pair#*:}; \
		ln -sfn "$(CURDIR)/target/debug/$$bin" "$(BINDIR)/$$alias"; \
		echo "$(BINDIR)/$$alias -> $(CURDIR)/target/debug/$$bin"; \
	done
	@# Whether BINDIR is on PATH, not whether some `mind` is findable: another
	@# one earlier on the PATH is exactly the case worth warning about.
	@case ":$$PATH:" in \
		*":$(BINDIR):"*) ;; \
		*) echo "warning: $(BINDIR) is not on your PATH, so these will not be found there" ;; \
	esac

.PHONY: dev/unlink
dev/unlink: ## Remove the dev/link symlinks (leaves installed copies alone)
	@for bin in $(BINS) $(foreach pair,$(ALIASES),$(firstword $(subst :, ,$(pair)))); do \
		target="$(BINDIR)/$$bin"; \
		if [ -L "$$target" ]; then \
			rm "$$target" && echo "removed $$target"; \
		elif [ -e "$$target" ]; then \
			echo "$$target is a real file, not a symlink: left alone"; \
			echo "(that is an installed copy; remove it with \`cargo uninstall mindflayer-cli\`)"; \
		else \
			echo "nothing to remove at $$target"; \
		fi; \
	done

.PHONY: dev/watch
dev/watch: ## Rebuild on every change, so the dev/link symlink is always current
	@command -v cargo-watch >/dev/null 2>&1 \
		|| { echo "cargo-watch is not installed: cargo install cargo-watch"; exit 1; }
	cargo watch -x 'build -p mindflayer-cli'

.PHONY: help
help: ## Show this help
	@echo "Mindflayer make targets:"
	@grep -hE '^[a-zA-Z0-9_/-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'
	@printf "  \033[36m%-12s\033[0m %s\n" "<app>/<t>" "run target <t> in an app (apps: $(APPS))"
