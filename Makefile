.PHONY: check fmt check-build clippy test run upstream official-build official-run official-smoke \
        official-interop-m1 official-interop-m2 rust-interop-m1 rust-interop-m2 m1-vectors

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

official-interop-m1:
	./scripts/official-node/interoperability.sh

official-interop-m2:
	RUST_INTEROP_REQUIRE_M2=1 ./scripts/official-node/interoperability.sh

rust-interop-m1:
	./scripts/rust-only-smoke.sh

rust-interop-m2:
	RUST_ONLY_TEST_NAME=full_rust_mock_peer_handshake_and_ping_pong ./scripts/rust-only-smoke.sh

m1-vectors:
	./scripts/generate-m1-vectors.sh
