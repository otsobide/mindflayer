.DEFAULT_GOAL := help

# Every unit of the product lives under apps/. Adding one (a desktop shell, a
# server backend) means a directory with its own Makefile and a line here.
APPS := core cli

# ---------------------------------------------------------------------------
# Per-app delegation — `make <app>/<target>` runs <target> in apps/<app>/Makefile
# e.g. `make core/test`, `make cli/run ARGS="list"`
# ---------------------------------------------------------------------------
define APP_DELEGATE
$(1)/%:
	$$(MAKE) -C apps/$(1) $$*
endef
$(foreach app,$(APPS),$(eval $(call APP_DELEGATE,$(app))))

# ---------------------------------------------------------------------------
# Global targets — CI invokes these, so they are the contract, not cargo.
#
# The aggregates are derived from APPS rather than spelled out, so that adding
# an app really is the one line above: a name left out of `test` would be an
# app CI silently never runs.
# ---------------------------------------------------------------------------
.PHONY: test
test: $(addsuffix /test,$(APPS)) ## Run every test suite

.PHONY: build
build: $(addsuffix /build,$(APPS)) ## Build every crate (debug)

.PHONY: fmt
fmt: ## Format all Rust code
	cargo fmt --all

.PHONY: fmt/check
fmt/check: ## Fail if any Rust code is not formatted
	cargo fmt --all --check

.PHONY: lint
lint: ## Run clippy across the workspace
	cargo clippy --workspace --all-targets

.PHONY: run
run: ## Run the CLI — pass args via ARGS="list"
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
# anything — that is what you want while working on them.
#
# Both binaries, not just `mind`: `flayer` is a second entry point into the
# same crate, and half an install is worse than none.
#
# "dev" is not in APPS, so these do not collide with the <app>/<target> pattern
# rules above. BINDIR governs the symlinks; `install` goes wherever cargo puts
# its binaries.
# ---------------------------------------------------------------------------
BINDIR ?= $(HOME)/.cargo/bin
BINS := mind flayer

.PHONY: install
install: ## Install `mind` and `flayer` as copies in cargo's bin directory
	cargo install --path apps/cli --locked

.PHONY: dev/link
dev/link: ## Build, then symlink `mind` and `flayer` into BINDIR
	cargo build -p mindflayer-cli
	@mkdir -p "$(BINDIR)"
	@for bin in $(BINS); do \
		ln -sfn "$(CURDIR)/target/debug/$$bin" "$(BINDIR)/$$bin"; \
		echo "$(BINDIR)/$$bin -> $(CURDIR)/target/debug/$$bin"; \
	done
	@# Whether BINDIR is on PATH, not whether some `mind` is findable: another
	@# one earlier on the PATH is exactly the case worth warning about.
	@case ":$$PATH:" in \
		*":$(BINDIR):"*) ;; \
		*) echo "warning: $(BINDIR) is not on your PATH, so these will not be found there" ;; \
	esac

.PHONY: dev/unlink
dev/unlink: ## Remove the dev/link symlinks (leaves installed copies alone)
	@for bin in $(BINS); do \
		target="$(BINDIR)/$$bin"; \
		if [ -L "$$target" ]; then \
			rm "$$target" && echo "removed $$target"; \
		elif [ -e "$$target" ]; then \
			echo "$$target is a real file, not a symlink: left alone"; \
			echo "(that is an installed copy — remove it with \`cargo uninstall mindflayer-cli\`)"; \
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
	@echo "Mindflayer — make targets:"
	@grep -hE '^[a-zA-Z0-9_/-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'
	@printf "  \033[36m%-12s\033[0m %s\n" "<app>/<t>" "run target <t> in an app — apps: $(APPS)"
