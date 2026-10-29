.DEFAULT_GOAL := build
.PHONY: build release test run fmt lint clean

# Without the panel first, so a build that leaves out the default-on `ui`
# feature is seen to build, and the binaries left in target/ are the default
# build's: cargo puts those back even when nothing of them needs building.
build: ## Build mindflayer-cli (binaries: mind, flayer), first without the panel
	cargo build -p mindflayer-cli --no-default-features
	cargo build -p mindflayer-cli

# The panel first, which the server embeds as it compiles: a release built
# without it would ship a `ui` that serves only a page saying so. It needs
# Node, as nothing else here does; PNPM reaches the panel's Makefile.
release: ## Release build for a Rust target triple (TARGET=aarch64-apple-darwin), the panel built first
ifndef TARGET
	$(error TARGET is required, e.g. make release TARGET=aarch64-apple-darwin)
endif
	$(MAKE) -C ../web build
	cargo build -p mindflayer-cli --release --locked --target $(TARGET)

test: ## Run mindflayer-cli tests
	cargo test -p mindflayer-cli

run: ## Run the CLI (pass args via ARGS="list")
	cargo run -p mindflayer-cli -- $(ARGS)

fmt: ## Format mindflayer-cli
	cargo fmt -p mindflayer-cli

lint: ## Clippy mindflayer-cli
	cargo clippy -p mindflayer-cli --all-targets

clean: ## Remove mindflayer-cli build artifacts
	cargo clean -p mindflayer-cli
