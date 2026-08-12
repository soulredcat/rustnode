# Alephium Rust Node

> Rust rebuild of the official Alephium node.

The objective is explicit: **build a working Alephium full node in Rust that can communicate and operate on the same network as the existing official Scala node.**

The first release is a compatibility implementation. It is not a new protocol, not a fork, and not an optimization experiment. The Rust node must first reproduce official Alephium behavior. Performance work begins only after compatibility is proven.

## Current status

**Milestone M2: request and decode real network data.**

Completed foundation:

- the minimum Rust workspace has previously compiled, linted, and tested in CI;
- CI can fetch the exact pinned official Alephium source;
- the official Scala node builds and runs as an isolated local devnet peer on `127.0.0.1:19985`;
- signed `Hello`, direct official-peer acceptance, `Ping/Pong`, and the M1 soak gate are complete;
- project tasks, mappings, AI rules, runbooks, and handoff documents are committed.

Current implementation includes basic M2 `Inv/Headers/Blocks/Txs` request/response and announcement handling, but the complete M2 acceptance gate is not closed. The exact next subphase is recorded in [`TODO.md`](./TODO.md). At the time this execution plan was introduced, the immediate work was restoring formatting/CI and making the official interoperability workflow reach the M2 harness.

**Full synchronization, consensus validation, BlockFlow parity, VM execution, persistent canonical state, and production use are not implemented yet.**

Do not use this repository for mainnet funds, mining, signing, or consensus decisions.

## Start here

Anyone—human, Codex, or another coding agent—must read these files in order:

1. [`README.md`](./README.md) — project mission and current status.
2. [`AGENTS.md`](./AGENTS.md) — mandatory implementation and agent rules.
3. [`TODO.md`](./TODO.md) — authoritative milestone/phase/subphase execution order and current pointer.
4. [`TASKS.md`](./TASKS.md) — detailed acceptance criteria, status narrative, and evidence.
5. [`ARCHITECTURE.md`](./ARCHITECTURE.md) — module boundaries and compatibility architecture.
6. [`docs/UPSTREAM_MAPPING.md`](./docs/UPSTREAM_MAPPING.md) — mapping from official Scala sources to Rust crates.
7. the milestone/runbook documents referenced by the active subphase.
8. [`docs/AI_HANDOFF.md`](./docs/AI_HANDOFF.md) — factual current handoff.
9. the latest relevant entry in [`docs/WORKLOG.md`](./docs/WORKLOG.md).

For a copy-paste coding-agent assignment, use [`docs/AI_SESSION_PROMPT.md`](./docs/AI_SESSION_PROMPT.md).

`TODO.md` decides **what is next**. `TASKS.md` decides **what evidence is required to call it done**. Do not silently work on a later milestone while an earlier compatibility gate remains open.

## Source of truth

Official implementation:

- Repository: https://github.com/alephium/alephium
- Pinned baseline commit: `63bcf6998508025f9538f3377629fe9e66df6c53`
- Baseline date: `2026-07-22`

The baseline is also recorded in [`UPSTREAM.toml`](./UPSTREAM.toml). During the initial rebuild, the Scala implementation and its tests define expected behavior.

When Rust and Scala disagree, do not guess. Reduce the discrepancy to a fixture, identify the relevant upstream code and test, and record the result.

## First completed interoperability objective

M1 established a live, accepted P2P session:

```text
Official Alephium Node (Scala)
          ^
          | official Alephium P2P
          v
     Rust Node
```

The Rust implementation can perform the M1 path:

1. load the selected Alephium network configuration;
2. load or create the identity required by the official handshake;
3. open a TCP connection to a locally controlled official node;
4. encode and decode the official message envelope;
5. exchange a valid signed `Hello`;
6. be accepted as an Alephium peer;
7. exchange `Ping` and `Pong`;
8. remain connected for the documented soak interval.

See [`docs/M1_P2P_INTEROP.md`](./docs/M1_P2P_INTEROP.md) for the gate and evidence requirements.

## Current objective

M2 must turn the accepted peer into a **decode-only network-data client** that reproducibly exchanges:

```text
ChainState / Inventory
          ↓
Headers request-response
          ↓
Blocks request-response
          ↓
Transactions request-response
          ↓
Announcement follow-up
```

Decoded data is not yet validated or committed canonical state.

## Development commands

```bash
# Clone the pinned official source into .upstream/alephium
make upstream

# Format, lint, compile, and test the current Rust workspace
make check

# Build and smoke-test the isolated official Scala node
make official-smoke

# WARNING: default local official-node ports are 19985/12973/10973.
# If Java node shares these, set OFFICIAL_NODE_P2P_PORT,
# OFFICIAL_NODE_REST_PORT, OFFICIAL_NODE_MINER_API_PORT, or RUST_INTEROP_PEER_PORT
# to other values before start.
# The M2 "all announcements" mode can be used with default settings:
# OFFICIAL_NODE_AUTO_MINE_FOR_DEV=1 and OFFICIAL_NODE_ENABLE_WALLET=1
# are automatically enabled by the interop script when RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS=all.

# Run the pure-Rust M1 bootstrap peer smoke (no Java/JVM dependency)
make rust-interop-m1

# Run the Rust/official M1 interoperability smoke
make official-interop-m1

# Generate official Scala M1 fixtures
make m1-vectors

# Full M2 interop (request/response markers, announcements optional)
make official-interop-m2-optional-announcements

# Full M2 interop (at least one announcement marker required)
make official-interop-m2-strict-announcements

# Full M2 interop (all announcement markers required)
make official-interop-m2-all-announcements

# Run the current Rust binary
make run

# Run node watcher with automatic reconnect
cargo run -p alephium-app -- --watch --peer <peer-address>

# Attach interop harness to an already-running peer
RUST_INTEROP_ATTACH_TO_EXISTING=1 make official-interop-m1
RUST_INTEROP_ATTACH_TO_EXISTING=1 RUST_INTEROP_REQUIRE_M2=1 make official-interop-m2
# Attach mode does not manage the official-node lifecycle.

# GitHub Actions manual-dispatch options:
# - m2_announcements=none   -> make official-interop-m2
# - m2_announcements=strict -> make official-interop-m2-strict-announcements
# - m2_announcements=all    -> make official-interop-m2-all-announcements
# - m2_announcements=warn   -> make official-interop-m2-optional-announcements
```

The current Rust binary must remain honest about which guarantees exist. With `--watch`, it retries connection when the peer is unavailable.

## Non-negotiable compatibility rules

Until compatibility is complete, do not change:

- BlockFlow consensus;
- PoW or PoLW behavior;
- P2P framing or payload semantics;
- transaction or block formats;
- hashes or signatures;
- Ralph/Alphred VM semantics;
- gas accounting;
- hard-fork behavior;
- group/shard behavior;
- block or transaction limits;
- tokenomics or fee distribution.

Do not use generic `bincode`, JSON, protobuf, or default Rust `serde` representations for official wire/consensus bytes. The repository crate named `alephium-serde` exists to reproduce Alephium's exact binary encoding.

## Workspace direction

The complete project should remain aligned with responsibilities in the official repository:

```text
macros
util
serde
io
crypto
protocol
flow
json
conf
api
ws
rpc
http
ralph
wallet
ralphc
app
tools
benchmark
integration
```

Only crates required for the active vertical slice should be added. Current conceptual graph:

```text
util
 |\
 | serde ---- crypto
 |    \       /
 |     protocol
 |        |
 conf ----+---- flow
                 |
                app
```

## Milestone order

```text
M0  repository/bootstrap/reference environment          DONE
 |
M1  official P2P handshake and Ping/Pong                DONE
 |
M2  request and decode real network data                ACTIVE
 |
M3  canonical protocol models, bytes, hashes, and IDs   BLOCKED
 |
M4  validation, mempool, PoW/PoLW, and BlockFlow        BLOCKED
 |
M5  Ralph/Alphred VM and logical world-state parity     BLOCKED
 |
M6  persistent storage and full synchronization         BLOCKED
 |
M7  required API/operations/full compatibility closure  BLOCKED
 |
M8  measured implementation optimization                BLOCKED
```

The complete phase/subphase tree is in [`TODO.md`](./TODO.md).

## Correctness evidence

For wire- or consensus-critical behavior, implementation is not complete without evidence. Depending on the subphase, evidence includes:

- official Scala test references;
- generated golden vectors;
- byte-for-byte encode/decode comparison;
- Rust/Scala P2P logs;
- differential validation results;
- VM result and gas comparison;
- historical replay and chain-tip/state-root comparison;
- crash/restart recovery evidence.

A benchmark does not prove compatibility.

## Documentation index

- [`TODO.md`](./TODO.md)
- [`TASKS.md`](./TASKS.md)
- [`ARCHITECTURE.md`](./ARCHITECTURE.md)
- [`AGENTS.md`](./AGENTS.md)
- [`CONTRIBUTING.md`](./CONTRIBUTING.md)
- [`AI_BUILD_PROMPT.md`](./AI_BUILD_PROMPT.md)
- [`docs/AI_SESSION_PROMPT.md`](./docs/AI_SESSION_PROMPT.md)
- [`docs/AI_HANDOFF.md`](./docs/AI_HANDOFF.md)
- [`docs/AI_TASK_TEMPLATE.md`](./docs/AI_TASK_TEMPLATE.md)
- [`docs/UPSTREAM_MAPPING.md`](./docs/UPSTREAM_MAPPING.md)
- [`docs/INTEROPERABILITY.md`](./docs/INTEROPERABILITY.md)
- [`docs/LOCAL_OFFICIAL_NODE.md`](./docs/LOCAL_OFFICIAL_NODE.md)
- [`docs/M1_WIRE_REFERENCE.md`](./docs/M1_WIRE_REFERENCE.md)
- [`docs/M1_P2P_INTEROP.md`](./docs/M1_P2P_INTEROP.md)
- [`docs/WORKLOG.md`](./docs/WORKLOG.md)
- [`tests/README.md`](./tests/README.md)

## License

No project license has been selected yet.

The official Alephium repository is licensed under LGPL-3.0. Licensing and attribution policy must be resolved before mechanically copying or translating substantial upstream source code. Protocol behavior may be studied and independently reimplemented, but an AI agent must not silently copy large upstream sections into this repository.

## Disclaimer

This project is experimental and unofficial until interoperability, historical replay, state parity, persistence/recovery, and security review have been demonstrated independently.
