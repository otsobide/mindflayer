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
# ---------------------------------------------------------------------------
.PHONY: test
test: core/test cli/test ## Run every test suite

.PHONY: build
build: core/build cli/build ## Build every crate (debug)

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
run: ## Run the CLI — pass args via ARGS="list --scope personal"
	$(MAKE) -C apps/cli run ARGS="$(ARGS)"

.PHONY: clean
clean: ## Remove build artifacts
	cargo clean

.PHONY: help
help: ## Show this help
	@echo "Mindflayer — make targets:"
	@grep -hE '^[a-zA-Z0-9_/-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'
	@echo "  \033[36m<app>/<t>\033[0m   run target <t> in an app — apps: $(APPS)"
