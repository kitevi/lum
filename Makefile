# Shortcuts for common cargo commands. `make install` is `cargo local-install`,
# which stays the canonical install (see xtask/README.md).
# Extra arguments go in ARGS: make run ARGS="radio list", make test ARGS=radio

.PHONY: help build release run test check clippy fmt clean install ci

help: ## List targets
	@grep -E '^[a-z]+:.*## ' $(MAKEFILE_LIST) | awk -F ':.*## ' '{ printf "  %-8s %s\n", $$1, $$2 }'

build: ## Debug build
	cargo build

release: ## Release build
	cargo build --release

run: ## Run lum with ARGS
	cargo run -- $(ARGS)

test: ## Run tests; ARGS filters them
	cargo test $(ARGS)

check: ## Type-check
	cargo check

clippy: ## Clippy with warnings as errors, as CI runs it
	cargo clippy -- -D warnings

fmt: ## Format the workspace
	cargo fmt --all

clean: ## Remove target/
	cargo clean

install: ## Install lum into your user bin dir (cargo local-install)
	cargo local-install

ci: clippy test ## What CI checks before a release
