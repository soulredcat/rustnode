# Fast-path interoperability artifacts

Run folder for `make official-interop-fastpath`. The harness builds the pinned
official node, starts it as an isolated devnet peer, runs the rustnode
against it, and asserts the official handshake and liveness exchange.

## Artifacts written here

- `commands.txt` — run manifest (`run_id`, commits, commands, durations, result).
- `rust-node.log` — Rust stdout from `alephium-app`.
- `error.txt` — Rust stderr from `alephium-app`.

The official node log stays at `.local/official-node/official-node.log`.

## Run

```bash
make official-interop-fastpath
```

To attach to an official node that is already running instead of letting the
script start and stop one:

```bash
RUST_INTEROP_ATTACH_TO_EXISTING=1 make official-interop-fastpath
```

Attach mode never starts or stops the official/Java process.

## Assertions

The run passes only when the Rust log contains both markers before the deadline:

- `received valid Hello` — the official Hello decoded and its broker signature verified;
- `received matching pong` — a Ping/Pong round trip completed.

Transaction-announcement interop (`NewTxHashes` → `TxsRequest` → raw transaction)
is not asserted yet; see `TODO.md`.

## Environment variables

- `RUST_INTEROP_PEER_ADDRESS` (default `127.0.0.1:19985`)
- `RUST_INTEROP_PEER_PORT` (default `19985`) — change only the port
- `RUST_INTEROP_DURATION_SECONDS` (default `120`)
- `RUST_INTEROP_ATTACH_TO_EXISTING` (`1` to skip official-node lifecycle management)
- `RUST_INTEROP_DIR` — artifact folder (default: this folder)
- `OFFICIAL_NODE_P2P_PORT`, `OFFICIAL_NODE_REST_PORT`, `OFFICIAL_NODE_MINER_API_PORT` — runtime config port overrides applied by `prepare-home.sh`
- `OFFICIAL_NODE_STATE_DIR`, `ALEPHIUM_UPSTREAM_DIR`
- `OFFICIAL_NODE_STARTUP_TIMEOUT_SECONDS` (default `180`)
