.PHONY: check fmt check-build clippy test run upstream

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
	cargo run -p alephium-app

upstream:
	./scripts/fetch-upstream.sh
