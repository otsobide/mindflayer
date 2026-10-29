.DEFAULT_GOAL := help

# Every unit of the product lives under apps/. Adding one (a desktop shell)
# means a directory with its own Makefile and a line here.
APPS := core cli server

# Apps built with a toolchain CI does not install yet. `make web/<target>`
# reaches them like any other app, but they stay out of the aggregates below
# until CI sets up Node and pnpm (#71): in there, `make test` would fail on
# every runner for want of pnpm, test-cross's macOS and Windows above all,
# and say nothing about the code.
NODE_APPS := web

# ---------------------------------------------------------------------------
# Per-app delegation: `make <app>/<target>` runs <target> in apps/<app>/Makefile
# e.g. `make core/test`, `make cli/run ARGS="list"`, `make web/build`
# ---------------------------------------------------------------------------
define APP_DELEGATE
$(1)/%:
	$$(MAKE) -C apps/$(1) $$*
endef
$(foreach app,$(APPS) $(NODE_APPS),$(eval $(call APP_DELEGATE,$(app))))

# ---------------------------------------------------------------------------
# Global targets: CI invokes these, so they are the contract, not cargo.
#
# The aggregates are derived from APPS rather than spelled out, so that adding
# an app really is the one line above: a name left out of `test` would be an
# app CI silently never runs.
# ---------------------------------------------------------------------------
.PHONY: test
test: $(addsuffix /test,$(APPS)) ## Run every Rust test suite; the panel's is web/test

.PHONY: build
build: $(addsuffix /build,$(APPS)) ## Build every crate (debug)

.PHONY: fmt
fmt: ## Format all Rust code
	cargo fmt --all

.PHONY: fmt/check
fmt/check: ## Fail if any Rust code is not formatted
	cargo fmt --all --check

# Twice: as everything builds, and with the `ts` feature, whose code (the
# panel's types and the tests holding them to serde) nothing else lints.
# Its own run rather than the one with the feature on, which would leave the
# build without it, the one every binary ships as, unlinted.
.PHONY: lint
lint: lint/em-dash ## Run clippy across the workspace, then with the ts feature, once no em dash is found
	cargo clippy --workspace --all-targets
	cargo clippy --workspace --all-targets --features mindflayer-core/ts,mindflayer-cli/ts

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
# every `make build` updates the binaries on your PATH without reinstalling
# anything, which is what you want while working on them.
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

# The panel first, as for a release: the copies embed it as they compile.
.PHONY: install
install: ## Install `mind` and `flayer` as copies in cargo's bin directory, the panel built first
	$(MAKE) -C apps/web build
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
	@printf "  \033[36m%-12s\033[0m %s\n" "<app>/<t>" "run target <t> in an app (apps: $(APPS) $(NODE_APPS))"
