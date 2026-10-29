.PHONY: check fmt check-build clippy test run mainnet-check m1-vectors

check: fmt check-build clippy test

fmt:
	cargo fmt --all -- --check

check-build:
	cargo check --workspace --all-targets

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

run:
	cargo run --release -p alephium-app -- --watch

mainnet-check:
	./scripts/mainnet-check.sh

m1-vectors:
	./scripts/generate-m1-vectors.sh
