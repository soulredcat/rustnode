.PHONY: check fmt check-build clippy test run upstream official-build official-run \
        official-interop-fastpath m1-vectors

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

upstream:
	./scripts/fetch-upstream.sh

official-build:
	./scripts/official-node/build.sh

official-run:
	./scripts/official-node/run.sh

official-interop-fastpath:
	./scripts/official-node/interoperability.sh

m1-vectors:
	./scripts/generate-m1-vectors.sh
