# Alephium Rust Node

> Rust rebuild of the official Alephium node.

The objective is explicit: **build a working Alephium full node in Rust that can communicate and operate on the same network as the existing official Scala node.**

The first release is a compatibility implementation. It is not a new protocol, not a fork, and not an optimization experiment. The Rust node must first reproduce official Alephium behavior. Performance work begins only after compatibility is proven.

## Current status

**Milestone M2: request and decode real network data (interoperability smoke level).**

Completed foundation:

- the minimum Rust workspace compiles, lints, and tests in CI;
- CI fetches the exact pinned official Alephium source;
- the official Scala node builds and runs as an isolated local devnet peer on `127.0.0.1:19973`;
- project tasks, upstream mappings, AI rules, runbooks, and worklogs are committed.

Current active work is extending interoperability smoke into basic sync requests (`Inv/Headers/Blocks/Txs`) and decoding the responses. **Rust synchronization state machine, validation, VM execution, and production use are not implemented yet.**

Do not use this repository for mainnet funds, mining, signing, or consensus decisions.

## Start here

Anyone—human or AI—working on this repository must read these files in order:

1. [`README.md`](./README.md) — project mission and milestone order.
2. [`AGENTS.md`](./AGENTS.md) — mandatory implementation rules and AI workflow.
3. [`TASKS.md`](./TASKS.md) — current task, completed work, and acceptance criteria.
4. [`ARCHITECTURE.md`](./ARCHITECTURE.md) — module boundaries and compatibility architecture.
5. [`docs/UPSTREAM_MAPPING.md`](./docs/UPSTREAM_MAPPING.md) — mapping from official Scala sources to Rust crates.
6. the milestone/runbook documents referenced by the current task.
7. [`docs/WORKLOG.md`](./docs/WORKLOG.md) — handoff record from previous work.

For a copy-paste coding-agent assignment, use [`docs/AI_SESSION_PROMPT.md`](./docs/AI_SESSION_PROMPT.md).

`TASKS.md` is the execution ledger. Do not silently work on a later milestone while an earlier compatibility gate remains open.

## Source of truth

Official implementation:

- Repository: https://github.com/alephium/alephium
- Pinned baseline commit: `63bcf6998508025f9538f3377629fe9e66df6c53`
- Baseline date: `2026-07-22`

The baseline is also recorded in [`UPSTREAM.toml`](./UPSTREAM.toml). During the initial rebuild, the Scala implementation and its tests define expected behavior.

When Rust and Scala disagree, do not guess. Reduce the discrepancy to a fixture, identify the relevant upstream code and test, and record the result.

## First useful objective

The first useful milestone is not a complete full node. It is a live, accepted P2P session:

```text
Official Alephium Node (Scala)
          ^
          | official Alephium P2P
          v
     Rust Node
```

The Rust binary must eventually:

1. load the selected Alephium network configuration;
2. load or create the identity required by the official handshake;
3. open a TCP connection to a locally controlled official node;
4. encode and decode the official message envelope;
5. exchange a valid signed `Hello`;
6. be accepted as an Alephium peer;
7. exchange `Ping` and `Pong`;
8. remain connected without an adapter or proxy.

See [`docs/M1_P2P_INTEROP.md`](./docs/M1_P2P_INTEROP.md) for the gate and required evidence.

## Development commands

```bash
# Clone the pinned official source into .upstream/alephium
make upstream

# Format, lint, compile, and test the current Rust workspace
make check

# Build and smoke-test the isolated official Scala node
make official-smoke

# Run the pure-Rust M1 bootstrap peer smoke (no Java / no JVM dependencies)
make rust-interop-m1

# Run the Rust/official interoperability smoke run
make official-interop-m1

# Generate official Scala M1 fixtures
make m1-vectors

# Run the current Rust bootstrap binary
make run

# Run node watcher (auto-retry reconnect, useful when peer starts later)
cargo run -p alephium-app -- --watch --peer <peer-address>
```

The current Rust binary prints an honest implementation-status banner.
With `--watch`, it keeps reconnecting and will connect to Alephium P2P when a peer becomes reachable.

## Non-negotiable initial rules

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

The complete project should remain aligned with the responsibilities in the official repository:

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

Only the minimum crates required for the current vertical slice are created initially. More crates are added when an accepted task requires them.

Current bootstrap graph:

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
M0  compiling repository bootstrap                 DONE
 |
M1  official P2P handshake and Ping/Pong           ACTIVE
 |
M2  request and decode real headers/blocks
 |
M3  protocol models, canonical bytes, hashes and IDs
 |
M4  validation and BlockFlow parity
 |
M5  Ralph/Alphred VM and world-state parity
 |
M6  persistent synchronization from genesis
 |
M7  required API/tooling parity
 |
M8  measured implementation optimization
```

At every milestone, the repository must remain runnable and must state honestly which guarantees are implemented.

## Correctness evidence

For wire- or consensus-critical behavior, implementation is not complete without evidence. Depending on the task, evidence includes:

- official Scala test references;
- generated golden vectors;
- byte-for-byte encode/decode comparison;
- Rust/Scala P2P logs;
- differential validation results;
- VM result and gas comparison;
- historical replay and chain-tip comparison.

A benchmark does not prove compatibility.

## Documentation index

- [`ARCHITECTURE.md`](./ARCHITECTURE.md)
- [`AGENTS.md`](./AGENTS.md)
- [`TASKS.md`](./TASKS.md)
- [`CONTRIBUTING.md`](./CONTRIBUTING.md)
- [`docs/AI_SESSION_PROMPT.md`](./docs/AI_SESSION_PROMPT.md)
- [`docs/AI_TASK_TEMPLATE.md`](./docs/AI_TASK_TEMPLATE.md)
- [`docs/UPSTREAM_MAPPING.md`](./docs/UPSTREAM_MAPPING.md)
- [`docs/LOCAL_OFFICIAL_NODE.md`](./docs/LOCAL_OFFICIAL_NODE.md)
- [`docs/M1_WIRE_REFERENCE.md`](./docs/M1_WIRE_REFERENCE.md)
- [`docs/M1_P2P_INTEROP.md`](./docs/M1_P2P_INTEROP.md)
- [`docs/WORKLOG.md`](./docs/WORKLOG.md)
- [`tests/README.md`](./tests/README.md)

## License

No project license has been selected yet.

The official Alephium repository is licensed under LGPL-3.0. Licensing and attribution policy must be resolved before mechanically copying or translating substantial upstream source code. Protocol behavior may be studied and independently reimplemented, but an AI agent must not silently copy large upstream sections into this repository.

## Disclaimer

This project is experimental and unofficial until interoperability, historical replay, state parity, and security review have been demonstrated independently.
