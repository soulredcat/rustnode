# M1 interoperability artifacts

Run folder for the official-node interoperability acceptance evidence required by
`docs/INTEROPERABILITY.md` and `docs/M1_P2P_INTEROP.md`.

This folder is intentionally ignored by Git for volatile logs, but the manifest can
be committed as a reproducible test record after each run.

## Required artifacts

- `commands.txt` — run manifest (`run_id`, commits, commands, durations).
- `rust-node.log` — Rust stdout capture from `alephium-app`.
- `error.txt` — Rust stderr capture from `alephium-app`.
- `official-node.log` (from `.local/official-node/official-node.log`) — linked in `commands.txt`.

## One-command local run

```bash
make official-interop-m1
make official-interop-m2
make official-interop-m2-optional-announcements
make official-interop-m2-strict-announcements
make official-interop-m2-all-announcements
```

Jika official node Anda sudah running di peer yang sama dan Anda tidak ingin script men-start/kill proses JVM, gunakan:

```bash
RUST_INTEROP_ATTACH_TO_EXISTING=1 make official-interop-m1
RUST_INTEROP_ATTACH_TO_EXISTING=1 RUST_INTEROP_REQUIRE_M2=1 make official-interop-m2
Peringatan: mode attach tidak akan menyalakan atau mematikan node official/Java yang sedang berjalan.
```

For a pure-Rust connectivity smoke (no official JVM node), use:

```bash
make rust-interop-m1
```

The command runs `crates/flow` integration test peer exchange (`full_rust_mock_peer_handshake_and_ping_pong`)
to validate the Rust handshake + ping/pong and frame decode path without Java.

- Environment variables:

- `RUST_INTEROP_PEER_ADDRESS` (default `127.0.0.1:19985`)
- `RUST_INTEROP_PEER_PORT` (default `19985`) - alternative shorthand to change only port
- `OFFICIAL_NODE_P2P_PORT` (default `19985`) - sets runtime official-node listener port when script-managed (`prepare-home.sh`)
- `OFFICIAL_NODE_REST_PORT` (default `12973`) - overrides official-node REST port in runtime copy (`prepare-home.sh`)
- `OFFICIAL_NODE_MINER_API_PORT` (default `10973`) - overrides official-node miner API port in runtime copy (`prepare-home.sh`)
- `RUST_INTEROP_DURATION_SECONDS` (default `120`)
- `OFFICIAL_NODE_STATE_DIR` / `RUST_INTEROP_DIR` / `ALEPHIUM_UPSTREAM_DIR`
- `OFFICIAL_NODE_STARTUP_TIMEOUT_SECONDS` (default `180`)
- `RUST_INTEROP_ATTACH_TO_EXISTING` (`1` to connect to already-running official peer and skip lifecycle management)
- `RUST_INTEROP_REQUIRE_M2` (`1` to enable M2 request/response assertions)
- `RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS`
  - `0` (default): no announcement checks
  - `1` or `strict`: strict mode (require at least one announcement marker)
  - `all`: require all announcement markers
  - `warn` or `optional`: log and continue if announcements are not observed (safe for local idle nodes)
  - `all` can also be used via `make official-interop-m2-all-announcements`.

The command runs:

1. official pinned-source build and local isolated setup;
2. official node startup wait for port readiness;
3. Rust flow run for the requested duration;
4. marker checks for `received valid Hello` and `received matching pong`;
   - with `official-interop-m2`, also checks `received ChainState`, request markers for
     `InvRequest`, `HeadersRequest`, `BlocksRequest`, `TxsRequest`, and response markers
     `HeadersResponse`, `BlocksResponse`, `TxsResponse`.
     If `ChainState` is absent, fallback headers request with empty locator is accepted as equivalent.
   - set `RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS=1`/`strict` to require at least one announcement marker in strict mode.
   - set `RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS=all` to require full announcement coverage:
     `NewBlock`, `NewHeader`, `NewInv`, `NewTxHashes`.
     equivalent to `make official-interop-m2-all-announcements`.
   - set `RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS=warn`/`optional` to run in optional mode on local-only peers.
5. process cleanup and output of `commands.txt` and log paths.

To force M2 checks in a local run, set:

```bash
RUST_INTEROP_REQUIRE_M2=1 make official-interop-m2
```
