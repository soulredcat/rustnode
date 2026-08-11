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
```

For a pure-Rust connectivity smoke (no official JVM node), use:

```bash
make rust-interop-m1
```

The command runs `crates/flow` integration test peer exchange (`full_rust_mock_peer_handshake_and_ping_pong`)
to validate the Rust handshake + ping/pong and frame decode path without Java.

Environment variables:

- `RUST_INTEROP_PEER_ADDRESS` (default `127.0.0.1:19973`)
- `RUST_INTEROP_DURATION_SECONDS` (default `120`)
- `OFFICIAL_NODE_STATE_DIR` / `RUST_INTEROP_DIR` / `ALEPHIUM_UPSTREAM_DIR`
- `OFFICIAL_NODE_STARTUP_TIMEOUT_SECONDS` (default `180`)

The command runs:

1. official pinned-source build and local isolated setup;
2. official node startup wait for port readiness;
3. Rust M1 flow run for the requested duration;
4. marker checks for `received valid Hello` and `received matching pong`;
5. process cleanup and output of `commands.txt` and log paths.
