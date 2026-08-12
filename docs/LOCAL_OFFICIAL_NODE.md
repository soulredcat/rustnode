# Local Official Alephium Node

## Purpose

This harness runs the pinned official Scala node as a **local isolated devnet peer** for Rust interoperability work.

It must be used for handshake, malformed-frame, and protocol experiments before any interaction with a public Alephium network.

Pinned source:

```text
https://github.com/alephium/alephium
63bcf6998508025f9538f3377629fe9e66df6c53
```

## Current support

The canonical harness targets Linux and WSL2 because that matches CI and allows one set of verified scripts.

Native PowerShell automation is not yet provided. On Windows, use WSL2 for M0/M1 work rather than manually translating commands and creating an unrecorded configuration variant.

## Local topology

```text
Official Scala node
network-id: 2 (devnet)
bootstrap peers: none
P2P: 127.0.0.1:19985
REST: 127.0.0.1:12973
miner API: 127.0.0.1:10973
P2P mode for M1: v1

          ^
          | direct TCP, no adapter
          v

Rust node under development
```

The official node is intentionally disconnected from mainnet/testnet discovery.

## Requirements

- Git
- Java 17 or newer
- sbt (required when rebuilding the official JAR)
- If `sbt` is unavailable and `.local/official-node/jar-path` already points to a valid
  official assembly JAR from the pinned commit, interoperability scripts can reuse that JAR.
- Bash
- Python 3 for readiness probe (optional; Windows bash can fall back to PowerShell socket probe when Python is unavailable)

CI currently verifies Java 17 with the pinned official source.

## One-command smoke test

```bash
make official-smoke
```

The command:

1. fetches the exact pinned official commit;
2. builds only `app/assembly`;
3. creates an isolated home under `.local/official-node/home`;
4. copies `config/official-node/user.conf` with mode `0600`;
5. starts the official JAR;
6. waits for `127.0.0.1:${OFFICIAL_NODE_P2P_PORT:-19985}`;
7. verifies the process remains alive after readiness;
8. prints recent logs;
9. shuts the process down through a cleanup trap.

Generated data and logs remain under `.local/official-node/` and are ignored by Git.

## Build only

```bash
make official-build
```

The selected assembly JAR path is written to:

```text
.local/official-node/jar-path
```

`make official-build` builds unless the JAR is already cached in
`.local/official-node/jar-path` and still valid. The script then reuses the cached JAR
and still fails if no unique matching JAR is available for that path.

## Run in foreground

```bash
make official-run
```

Stop with `Ctrl+C`.

The run command preserves the existing isolated database/identity. To reset it first:

```bash
OFFICIAL_NODE_RESET=1 make official-run
```

## Configuration ownership

Committed source configuration:

```text
config/official-node/user.conf
```

Runtime copy:

```text
.local/official-node/home/user.conf
```

Important choices:

- `network-id = 2` selects devnet;
- one broker covers four groups;
- discovery bootstrap is empty;
- all listeners bind to loopback;
- UPnP is disabled;
- wallet is disabled;
- API key is disabled only because REST binds to loopback;
- P2P v2 is disabled until Rust implements and verifies it;
- the official ping interval is reduced to five seconds for M1 liveness tests.
- `OFFICIAL_NODE_AUTO_MINE_FOR_DEV` enables local auto-mining in the copied runtime config
  (`mempool.auto-mine-for-dev = true`) when full-announcement interop is requested;
- `OFFICIAL_NODE_ENABLE_WALLET` enables wallet APIs in the copied runtime config
  (`wallet.enabled = true`) when full-announcement interop requires transaction seeding.

`OFFICIAL_NODE_P2P_PORT`, `OFFICIAL_NODE_REST_PORT`, and
`OFFICIAL_NODE_MINER_API_PORT` override the copied runtime config’s listener ports in
the `prepare-home.sh` step.

Do not change the runtime copy manually. Change the committed source config, document the reason, then rerun the smoke workflow.

## Logs

Smoke log:

```text
.local/official-node/official-node.log
```

Before committing interoperability evidence:

- inspect for secrets/private keys;
- redact machine-specific paths where necessary;
- preserve timestamps, peer state transitions, client IDs, and protocol errors required by the test.

## Safety boundary

Do not point malformed-frame tests at public mainnet or testnet peers.

WARNING: this repository’s local interoperability harness is configured to use
`19985`/`12973`/`10973` by default.
If Java/Alephium uses these ports, override with `OFFICIAL_NODE_P2P_PORT`,
`OFFICIAL_NODE_REST_PORT`, `OFFICIAL_NODE_MINER_API_PORT`, or
`RUST_INTEROP_PEER_PORT` before start.
Jangan menjalankan dua node yang bind ke `19985` secara bersamaan; gunakan
`RUST_INTEROP_ATTACH_TO_EXISTING=1` saat hanya ingin attach ke Java peer yang sudah jalan.

Do not expose ports `19985`, `12973`, or `10973` beyond loopback for this harness.

Do not reuse the harness home for wallets, mining, or production state.

## M0-004 acceptance

M0-004 is complete only when the GitHub Actions `official-node-smoke` workflow builds the pinned application and proves the loopback P2P listener opens successfully. A written runbook without a successful smoke run is not sufficient.
