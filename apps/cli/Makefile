.DEFAULT_GOAL := build
.PHONY: build release test run fmt lint clean

build: ## Build mindflayer-cli (binaries: mind, flayer)
	cargo build -p mindflayer-cli

release: ## Release build for a Rust target triple (TARGET=aarch64-apple-darwin)
ifndef TARGET
	$(error TARGET is required, e.g. make release TARGET=aarch64-apple-darwin)
endif
	cargo build -p mindflayer-cli --release --locked --target $(TARGET)

test: ## Run mindflayer-cli tests
	cargo test -p mindflayer-cli

run: ## Run the CLI — pass args via ARGS="list"
	cargo run -p mindflayer-cli -- $(ARGS)

fmt: ## Format mindflayer-cli
	cargo fmt -p mindflayer-cli

lint: ## Clippy mindflayer-cli
	cargo clippy -p mindflayer-cli --all-targets

clean: ## Remove mindflayer-cli build artifacts
	cargo clean -p mindflayer-cli
