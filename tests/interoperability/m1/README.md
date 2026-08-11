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

- `RUST_INTEROP_PEER_ADDRESS` (default `127.0.0.1:19974`)
- `RUST_INTEROP_PEER_PORT` (default `19974`) - alternative shorthand to change only port
- `RUST_INTEROP_DURATION_SECONDS` (default `120`)
- `OFFICIAL_NODE_STATE_DIR` / `RUST_INTEROP_DIR` / `ALEPHIUM_UPSTREAM_DIR`
- `OFFICIAL_NODE_STARTUP_TIMEOUT_SECONDS` (default `180`)
- `RUST_INTEROP_ATTACH_TO_EXISTING` (`1` to connect to already-running official peer and skip lifecycle management)

The command runs:

1. official pinned-source build and local isolated setup;
2. official node startup wait for port readiness;
3. Rust flow run for the requested duration;
4. marker checks for `received valid Hello` and `received matching pong`;
   - with `official-interop-m2`, also checks `Inv/Headers/Blocks/Txs` request and response markers.
5. process cleanup and output of `commands.txt` and log paths.

To force M2 checks in a local run, set:

```bash
RUST_INTEROP_REQUIRE_M2=1 make official-interop-m2
```
