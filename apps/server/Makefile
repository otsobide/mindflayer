.DEFAULT_GOAL := build
.PHONY: build release test fmt lint clean

build: ## Build mindflayer-server
	cargo build -p mindflayer-server

# The optimized build, which CI's `build (server)` runs once the debug build
# has passed, as core's is. It embeds whatever apps/web/dist holds, nothing
# in that job, which `allow_missing` lets it build without: the release that
# has to carry the panel is the CLI's, which `make cli/release/check` holds
# to serving it.
release: ## Release (optimized) build of mindflayer-server
	cargo build -p mindflayer-server --release --locked

test: ## Run mindflayer-server tests
	cargo test -p mindflayer-server

fmt: ## Format mindflayer-server
	cargo fmt -p mindflayer-server

lint: ## Clippy mindflayer-server
	cargo clippy -p mindflayer-server --all-targets

clean: ## Remove mindflayer-server build artifacts
	cargo clean -p mindflayer-server
