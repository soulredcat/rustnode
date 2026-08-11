.PHONY: check fmt check-build clippy test run upstream official-build official-run official-smoke m1-vectors

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

official-build:
	./scripts/official-node/build.sh

official-run:
	./scripts/official-node/run.sh

official-smoke:
	./scripts/official-node/smoke.sh

m1-vectors:
	./scripts/generate-m1-vectors.sh
