.DEFAULT_GOAL := build
.PHONY: build test fmt lint clean

build: ## Build mindflayer-core
	cargo build -p mindflayer-core

test: ## Run mindflayer-core tests
	cargo test -p mindflayer-core

fmt: ## Format mindflayer-core
	cargo fmt -p mindflayer-core

lint: ## Clippy mindflayer-core
	cargo clippy -p mindflayer-core --all-targets

clean: ## Remove mindflayer-core build artifacts
	cargo clean -p mindflayer-core
