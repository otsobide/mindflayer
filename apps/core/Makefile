.DEFAULT_GOAL := build
.PHONY: build release test fmt lint clean

build: ## Build mindflayer-core
	cargo build -p mindflayer-core

# The optimized build, which CI's `build (core)` runs once the debug build
# has passed, so a crate that compiles only in debug fails before a release
# is cut from it. `--locked`, as the CLI's release is: a lockfile the build
# would change fails rather than is rewritten.
release: ## Release (optimized) build of mindflayer-core
	cargo build -p mindflayer-core --release --locked

test: ## Run mindflayer-core tests
	cargo test -p mindflayer-core

fmt: ## Format mindflayer-core
	cargo fmt -p mindflayer-core

lint: ## Clippy mindflayer-core
	cargo clippy -p mindflayer-core --all-targets

clean: ## Remove mindflayer-core build artifacts
	cargo clean -p mindflayer-core
