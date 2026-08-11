#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

(
  cd "${ROOT_DIR}"
  cargo test -p alephium-flow --test rust_peer_smoke -- --exact full_rust_mock_peer_handshake_and_ping_pong
)
