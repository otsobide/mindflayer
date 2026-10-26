.DEFAULT_GOAL := build
.PHONY: build test fmt lint clean

build: ## Build mindflayer-server
	cargo build -p mindflayer-server

test: ## Run mindflayer-server tests
	cargo test -p mindflayer-server

fmt: ## Format mindflayer-server
	cargo fmt -p mindflayer-server

lint: ## Clippy mindflayer-server
	cargo clippy -p mindflayer-server --all-targets

clean: ## Remove mindflayer-server build artifacts
	cargo clean -p mindflayer-server
