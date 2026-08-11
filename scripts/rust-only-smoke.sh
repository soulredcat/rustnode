#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_NAME="${RUST_ONLY_TEST_NAME:-full_rust_mock_peer_handshake_and_ping_pong}"

(
  cd "${ROOT_DIR}"
  cargo test -p alephium-flow --test rust_peer_smoke -- --exact "${TEST_NAME}"
)
