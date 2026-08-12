# TODO — Ordered Alephium Rust Node Rebuild

Last updated: `2026-08-12`

This file defines the **mandatory execution order** for humans, Codex, and other coding agents.

The goal remains unchanged:

> Build a Rust full-node implementation that is protocol- and consensus-compatible with the pinned official Alephium Scala node before attempting optimization or protocol redesign.

## 1. Authority and document roles

Use the following precedence:

1. **Pinned official Alephium Scala implementation** — behavioral source of truth.
2. **`TODO.md`** — milestone, phase, subphase, dependency, and execution order.
3. **`TASKS.md`** — detailed acceptance criteria, status narrative, and evidence ledger.
4. **`ARCHITECTURE.md`** — module boundaries and dependency direction.
5. **`docs/UPSTREAM_MAPPING.md`** — upstream Scala-to-Rust ownership mapping.
6. **`docs/WORKLOG.md` and `docs/AI_HANDOFF.md`** — factual session history and current handoff.

When documents disagree:

- official observable behavior wins over every project document;
- `TODO.md` wins for **what must be done next**;
- `TASKS.md` wins for **what evidence is required to call it done**;
- fix the disagreement in the same change instead of silently choosing one.

## 2. Status notation

- `[x]` — complete; acceptance evidence exists.
- `[ ]` — incomplete.
- **`[ACTIVE]`** — the only subphase allowed to receive implementation work now.
- **`[VERIFY]`** — implementation exists, but its acceptance gate is not fully green.
- **`[BLOCKED]`** — dependencies are incomplete.

Exactly one subphase should be marked `[ACTIVE]`.

A parent phase or milestone is complete only when every required child is `[x]` and its exit gate passes.

## 3. Current execution pointer

```text
Milestone: M2 — Request and decode real network data
Phase:     M2-P0 — Restore a green, reproducible M2 baseline
Subphase:  M2-P0-S1 — Apply rustfmt and restore the local formatting gate
Status:    ACTIVE
```

Current factual state before this plan was added:

- M0 is complete.
- M1 direct Rust-to-official-Scala handshake and Ping/Pong interoperability is complete.
- M2 request/response implementation exists in part and requires verification/closure.
- the latest Rust CI run stopped at `cargo fmt --all -- --check`;
- the latest official interoperability workflow stopped before the harness because `.upstream/alephium` was referenced by Java cache setup before the pinned upstream checkout existed;
- M3 through M8 remain blocked.

Do not start M3 until the complete M2 exit gate is checked.

## 4. Deterministic Codex task-selection algorithm

At the beginning of every coding session, Codex must:

1. Read `README.md`, `AGENTS.md`, this file, `TASKS.md`, and `ARCHITECTURE.md`.
2. Read `docs/UPSTREAM_MAPPING.md`, `docs/AI_HANDOFF.md`, and the latest `docs/WORKLOG.md` entry.
3. Inspect Git status, current HEAD, recent commits, and CI state.
4. Select the exact subphase named by **Current execution pointer**.
5. Verify that all declared dependencies are `[x]`.
6. Inspect the relevant pinned official Scala source and tests before changing protocol behavior.
7. Work only on that subphase; do not bundle later subphases because they are nearby.
8. Run the subphase acceptance commands and the repository-wide gates.
9. Mark the subphase `[x]` only when required evidence actually exists.
10. Move **Current execution pointer** to the first incomplete unblocked subphase.
11. Update `TASKS.md`, `docs/WORKLOG.md`, and `docs/AI_HANDOFF.md` in the same session.

When blocked:

- record the exact blocker and reproduction;
- complete only independent work explicitly allowed by the active subphase;
- leave the checkbox open;
- do not jump to another milestone unless this file is intentionally revised by a human or a reviewed planning change.

## 5. Global completion gates

Unless a subphase states otherwise, run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Networking phases must also run their direct official-node interoperability command.

Wire-, crypto-, protocol-, validation-, VM-, and state-critical phases require at least one of:

- Scala-generated golden vectors;
- a differential Scala/Rust test;
- a captured frame from a locally controlled pinned official node;
- historical replay comparison;
- direct official-node interoperability evidence.

Rust-only round trips are not sufficient parity evidence.

---

# M0 — Repository and compatibility foundation

**Milestone status:** `[x] DONE`

## M0-P0 — Project control plane

- [x] **M0-P0-S1** Define compatibility-first mission and pinned upstream baseline.
- [x] **M0-P0-S2** Add architecture, agent rules, task ledger, worklog, and handoff documents.
- [x] **M0-P0-S3** Add reusable AI/Codex prompts and contribution rules.

## M0-P1 — Rust workspace bootstrap

- [x] **M0-P1-S1** Create minimum Cargo workspace and crate dependency graph.
- [x] **M0-P1-S2** Add formatting, check, Clippy, and test commands.
- [x] **M0-P1-S3** Establish green Rust CI on the supported stable toolchain.

## M0-P2 — Official reference environment

- [x] **M0-P2-S1** Pin official Alephium source commit.
- [x] **M0-P2-S2** Add reproducible pinned-upstream acquisition script.
- [x] **M0-P2-S3** Build and start an isolated official Scala devnet node.
- [x] **M0-P2-S4** Record loopback ports, commands, logs, cleanup, and CI smoke evidence.

## M0 exit gate

- [x] Rust workspace is green.
- [x] Pinned official source is reproducibly available.
- [x] Isolated official node opens its local P2P listener.

---

# M1 — Official P2P handshake and liveness

**Milestone status:** `[x] DONE`

## M1-P0 — Official M1 behavior freeze

- [x] **M1-P0-S1** Map official message envelope, payload codes, compact integers, identity, and version parsing.
- [x] **M1-P0-S2** Generate canonical `Hello`, `Ping`, `Pong`, malformed, and truncation vectors from Scala.
- [x] **M1-P0-S3** Commit vector provenance and reproducibility tooling.

## M1-P1 — Alephium codec foundation

- [x] **M1-P1-S1** Implement bounded fixed-width and compact integer codecs.
- [x] **M1-P1-S2** Implement bounded strings, byte vectors, products, and exact trailing-byte behavior required by M1.
- [x] **M1-P1-S3** Implement network magic, DJB checksum, frame length, payload header, and incremental TCP framing.

## M1-P2 — Identity and protocol models

- [x] **M1-P2-S1** Implement timestamp, request ID, wire/P2P/release version models.
- [x] **M1-P2-S2** Implement clique and broker identity structures.
- [x] **M1-P2-S3** Implement official SecP256K1 key, signature, signing, and verification formats required by `Hello`.
- [x] **M1-P2-S4** Implement atomic identity create/load persistence.

## M1-P3 — Handshake state machine

- [x] **M1-P3-S1** Encode/decode and validate signed `Hello`.
- [x] **M1-P3-S2** Implement bounded TCP connection and handshake states.
- [x] **M1-P3-S3** Implement `Ping`/`Pong`, request correlation, deadlines, EOF, and clean shutdown.

## M1-P4 — Direct official interoperability

- [x] **M1-P4-S1** Connect directly to the pinned official Scala node without a translating proxy.
- [x] **M1-P4-S2** Prove official peer acceptance and valid bidirectional `Hello`.
- [x] **M1-P4-S3** Prove `Ping`/`Pong` and the required soak interval.
- [x] **M1-P4-S4** Commit commands and evidence.

## M1 exit gate

- [x] Official Scala node accepts the Rust peer.
- [x] Signed `Hello` and frame bytes match official behavior.
- [x] Liveness and soak evidence are reproducible.

---

# M2 — Request and decode real network data

**Milestone status:** `[ ] IN PROGRESS`

M2 proves transport and decoding. It must not describe decoded data as consensus-validated.

## M2-P0 — Restore a green, reproducible M2 baseline

- [ ] **M2-P0-S1 `[ACTIVE]`** Apply `cargo fmt --all`; review that changes are formatting-only; rerun `cargo fmt --all -- --check`.
- [ ] **M2-P0-S2 `[BLOCKED: M2-P0-S1]`** Run workspace check, Clippy, and all tests; repair real compile/lint/test failures without expanding scope.
- [ ] **M2-P0-S3 `[BLOCKED: M2-P0-S2]`** Fix official interoperability workflow ordering so pinned upstream is fetched before Java/SBT cache paths are evaluated; ensure early failures still leave a useful error artifact.
- [ ] **M2-P0-S4 `[BLOCKED: M2-P0-S3]`** Obtain green Rust CI and a workflow run that reaches the official interoperability harness.

## M2-P1 — Freeze official sync-message behavior

- [ ] **M2-P1-S1 `[VERIFY]`** Inventory every M2 payload code and field order used by `ChainState`, `Inv`, `Headers`, `Blocks`, `Txs`, and announcements.
- [ ] **M2-P1-S2 `[VERIFY]`** Generate or capture provenance-backed valid payload fixtures from the pinned official implementation.
- [ ] **M2-P1-S3** Add malformed, truncation, excessive-length, invalid request-ID, and trailing-byte fixtures.
- [ ] **M2-P1-S4** Update `docs/UPSTREAM_MAPPING.md` with exact Scala symbols and Rust owners.

## M2-P2 — Inventory and chain-state exchange

- [ ] **M2-P2-S1 `[VERIFY]`** Verify canonical encode/decode for `ChainState`, `InvRequest`, `InvResponse`, and locators.
- [ ] **M2-P2-S2 `[VERIFY]`** Verify request-ID correlation, empty/non-empty inventory behavior, and chain/group dimensions.
- [ ] **M2-P2-S3** Add bounded retry, timeout, duplicate, unsolicited-response, and disconnect tests.
- [ ] **M2-P2-S4** Capture direct official-node inventory/chain-state evidence.

## M2-P3 — Header transfer

- [ ] **M2-P3-S1 `[VERIFY]`** Verify `HeadersRequest`, `HeadersResponse`, height-based header messages, and official raw-header boundaries.
- [ ] **M2-P3-S2** Preserve canonical raw header bytes without claiming header validation.
- [ ] **M2-P3-S3** Verify empty, multiple, partial, duplicate, and oversized response behavior.
- [ ] **M2-P3-S4** Capture direct official-node request/response evidence and decoded counts.

## M2-P4 — Block transfer

- [ ] **M2-P4-S1 `[VERIFY]`** Verify `BlocksRequest`, `BlocksResponse`, blocks-and-uncles-by-height messages, and official raw-block boundaries.
- [ ] **M2-P4-S2** Preserve canonical raw block bytes without claiming transaction or block validation.
- [ ] **M2-P4-S3** Verify empty, multiple, partial, duplicate, and oversized response behavior.
- [ ] **M2-P4-S4** Capture direct official-node request/response evidence and decoded counts.

## M2-P5 — Transaction transfer and announcements

- [ ] **M2-P5-S1 `[VERIFY]`** Verify `TxsRequest`, `TxsResponse`, and transaction-hash grouping by chain.
- [ ] **M2-P5-S2 `[VERIFY]`** Verify `NewInv`, `NewHeader`, `NewBlockHash`, `NewBlock`, and `NewTxHashes` decoding.
- [ ] **M2-P5-S3** Verify follow-up requests generated from announcements and prevent duplicate/unbounded requests.
- [ ] **M2-P5-S4** Capture direct official-node transaction and announcement evidence where reproducible.

## M2-P6 — Decode-only sync session behavior

- [ ] **M2-P6-S1** Split the current large flow implementation into clear session, dispatcher, request tracker, sync probe, and decoded-state modules without changing behavior.
- [ ] **M2-P6-S2** Bound inbound buffers, outstanding request maps, retries, decoded-object retention, and announcement deduplication.
- [ ] **M2-P6-S3** Implement deterministic reconnect/resume of the decode-only probe state.
- [ ] **M2-P6-S4** Add cancellation, timeout, peer disconnect, malformed-message, and multi-frame tests.

## M2-P7 — M2 exit gate

- [ ] **M2-P7-S1** All global Rust gates pass.
- [ ] **M2-P7-S2** `make official-interop-m2` reaches and verifies inventory, header, block, and transaction request/response markers.
- [ ] **M2-P7-S3** Announcement modes are documented as required/optional according to reproducibility, without false completion claims.
- [ ] **M2-P7-S4** Evidence, run IDs, commands, known limitations, `TASKS.md`, mapping, worklog, and handoff are updated.
- [ ] **M2-P7-S5** Mark M2 complete and move the execution pointer to `M3-P0-S1`.

---

# M3 — Canonical protocol models, serialization, hashes, and IDs

**Milestone status:** `[ ] BLOCKED BY M2`

## M3-P0 — Extend the official differential oracle

- [ ] **M3-P0-S1** Map official transaction, block-header, block, address, script, asset, group, chain, hash, and hard-fork types.
- [ ] **M3-P0-S2** Build Scala fixture generators for canonical valid objects and boundary cases.
- [ ] **M3-P0-S3** Generate malformed structural fixtures and canonical hash/ID outputs.
- [ ] **M3-P0-S4** Add a reproducible Rust-vs-Scala vector comparison command.

## M3-P1 — Core protocol primitives

- [ ] **M3-P1-S1** Implement fixed hash/ID types and exact byte/display parsing.
- [ ] **M3-P1-S2** Implement group index, chain index, group-pair mapping, and bounds.
- [ ] **M3-P1-S3** Implement amounts, tokens, timestamps, targets, nonces, and protocol integer types required by blocks/transactions.
- [ ] **M3-P1-S4** Prove canonical byte parity for every primitive.

## M3-P2 — Addresses, scripts, assets, and references

- [ ] **M3-P2-S1** Implement lock/unlock scripts and address derivation/encoding used by transaction models.
- [ ] **M3-P2-S2** Implement asset/token collections with official ordering and bounds.
- [ ] **M3-P2-S3** Implement transaction output references and related keys.
- [ ] **M3-P2-S4** Add official valid/invalid fixtures and round-trip plus byte-parity tests.

## M3-P3 — Transaction model and transaction ID

- [ ] **M3-P3-S1** Implement unsigned transaction structures and canonical serialization.
- [ ] **M3-P3-S2** Implement signatures/scripts and full transaction structures.
- [ ] **M3-P3-S3** Implement transaction ID/hash preimage exactly as official.
- [ ] **M3-P3-S4** Differential-test canonical bytes and IDs across representative and boundary transactions.

## M3-P4 — Block header and dependencies

- [ ] **M3-P4-S1** Implement dependency vectors, header fields, target, nonce, timestamp, and version.
- [ ] **M3-P4-S2** Implement canonical header serialization and header hash.
- [ ] **M3-P4-S3** Implement chain/group derivation from official header/hash semantics.
- [ ] **M3-P4-S4** Differential-test bytes, hashes, dependency ordering, and malformed cases.

## M3-P5 — Block model and block hash

- [ ] **M3-P5-S1** Implement block body, transactions, uncles/ghost data where applicable, and canonical ordering.
- [ ] **M3-P5-S2** Implement canonical block serialization and block identity semantics.
- [ ] **M3-P5-S3** Decode M2 raw objects into complete typed protocol objects.
- [ ] **M3-P5-S4** Differential-test official blocks and malformed structures.

## M3-P6 — Network and hard-fork model

- [ ] **M3-P6-S1** Centralize network IDs, genesis parameters, groups, and protocol version constants.
- [ ] **M3-P6-S2** Implement official hard-fork enumeration and activation selection.
- [ ] **M3-P6-S3** Remove scattered timestamp/version guesses from protocol code.
- [ ] **M3-P6-S4** Add exact boundary vectors for every supported hard fork.

## M3-P7 — M3 exit gate

- [ ] **M3-P7-S1** Scala and Rust canonical transaction bytes and IDs match.
- [ ] **M3-P7-S2** Scala and Rust canonical header/block bytes and hashes match.
- [ ] **M3-P7-S3** All malformed and boundary fixtures agree.
- [ ] **M3-P7-S4** Global gates and differential suite pass; evidence is recorded.

---

# M4 — Validation, mempool rules, PoW/PoLW, and BlockFlow parity

**Milestone status:** `[ ] BLOCKED BY M3`

## M4-P0 — Validation oracle and error taxonomy

- [ ] **M4-P0-S1** Map official stateless/stateful transaction, header, block, PoW/PoLW, weight, and BlockFlow validators.
- [ ] **M4-P0-S2** Generate accepted/rejected Scala fixtures with expected failure categories.
- [ ] **M4-P0-S3** Define typed Rust validation errors without relying on strings for control flow.
- [ ] **M4-P0-S4** Build a differential validation harness.

## M4-P1 — Stateless transaction validation

- [ ] **M4-P1-S1** Validate structure, counts, sizes, references, signatures, script shape, amounts, token collections, and gas fields.
- [ ] **M4-P1-S2** Implement official overflow, duplicate, ordering, and malformed-data rejection behavior.
- [ ] **M4-P1-S3** Differential-test accepted/rejected transactions.

## M4-P2 — Stateful transaction validation

- [ ] **M4-P2-S1** Define read-only validation views over UTXO/contract state.
- [ ] **M4-P2-S2** Validate input existence, spendability, asset conservation, contract preconditions, and conflict rules.
- [ ] **M4-P2-S3** Differential-test state-dependent accepted/rejected transactions.

## M4-P3 — Header, PoW/PoLW, target, and weight validation

- [ ] **M4-P3-S1** Implement target/difficulty and PoW verification.
- [ ] **M4-P3-S2** Implement PoLW-specific checks and official fork behavior.
- [ ] **M4-P3-S3** Implement timestamp, dependency, version, chain/group, and weight rules.
- [ ] **M4-P3-S4** Differential-test valid/invalid headers and boundary targets.

## M4-P4 — Block validation

- [ ] **M4-P4-S1** Validate header/body linkage, transactions, ordering, dependencies, uncles, and block limits.
- [ ] **M4-P4-S2** Validate cumulative state effects using a deterministic temporary state view.
- [ ] **M4-P4-S3** Differential-test valid and invalid official blocks.

## M4-P5 — BlockFlow graph and canonical selection

- [ ] **M4-P5-S1** Implement chain/group views, dependency graph, tips, locators, and weight comparison.
- [ ] **M4-P5-S2** Implement insertion, orphan handling, competing branches, reorg decisions, and cache invalidation.
- [ ] **M4-P5-S3** Differential-test official BlockFlow decisions over deterministic block sequences.

## M4-P6 — Mempool parity

- [ ] **M4-P6-S1** Implement pending/ready/conflict states and official admission rules.
- [ ] **M4-P6-S2** Implement dependency ordering, replacement/removal, block inclusion, and reorg reinsertion.
- [ ] **M4-P6-S3** Bound memory, queues, validation work, and peer-driven churn.
- [ ] **M4-P6-S4** Differential-test representative mempool transitions.

## M4-P7 — M4 exit gate

- [ ] **M4-P7-S1** Rust accepts and rejects the same transaction/header/block fixture set as Scala.
- [ ] **M4-P7-S2** Rust derives the same BlockFlow tips/decisions for deterministic histories.
- [ ] **M4-P7-S3** Mempool transitions agree for supported scenarios.
- [ ] **M4-P7-S4** Global gates, differential validation, and evidence pass.

---

# M5 — Ralph/Alphred VM and logical world-state parity

**Milestone status:** `[ ] BLOCKED BY M4`

## M5-P0 — VM specification and oracle

- [ ] **M5-P0-S1** Map official values, instructions, frames, context, gas schedule, errors, contract/asset behavior, and hard-fork branches.
- [ ] **M5-P0-S2** Build Scala-generated stateless/stateful VM fixtures.
- [ ] **M5-P0-S3** Record result values, errors, gas, outputs, events, and state diffs.
- [ ] **M5-P0-S4** Build differential VM runner.

## M5-P1 — Values, instructions, and codec

- [ ] **M5-P1-S1** Implement VM value types and exact bounds/overflow behavior.
- [ ] **M5-P1-S2** Implement instruction/opcode decoding and canonical encoding.
- [ ] **M5-P1-S3** Implement program/method metadata and code-size rules per hard fork.
- [ ] **M5-P1-S4** Differential-test instruction/value serialization.

## M5-P2 — Execution engine

- [ ] **M5-P2-S1** Implement operand stack, call stack, frames, locals/fields, and control flow.
- [ ] **M5-P2-S2** Implement arithmetic, crypto, byte, collection, and environment instructions.
- [ ] **M5-P2-S3** Implement stateless execution and deterministic error ordering.
- [ ] **M5-P2-S4** Differential-test results and failures.

## M5-P3 — Gas accounting

- [ ] **M5-P3-S1** Implement instruction, memory, code, call, storage, and asset gas costs.
- [ ] **M5-P3-S2** Implement exact charge timing and out-of-gas behavior.
- [ ] **M5-P3-S3** Centralize fork-dependent gas schedules.
- [ ] **M5-P3-S4** Differential-test gas on every fixture class.

## M5-P4 — Stateful contracts and assets

- [ ] **M5-P4-S1** Implement contract creation/load/call/destroy lifecycle.
- [ ] **M5-P4-S2** Implement mutable/immutable fields and subcontract semantics.
- [ ] **M5-P4-S3** Implement payable/non-payable calls, balances, tokens, approvals, and generated outputs.
- [ ] **M5-P4-S4** Implement events/logical logs and state diff production.
- [ ] **M5-P4-S5** Differential-test stateful execution.

## M5-P5 — Logical world state and Sparse Merkle Trie semantics

- [ ] **M5-P5-S1** Implement logical asset/contract state interfaces independent of physical database choice.
- [ ] **M5-P5-S2** Implement official Sparse Merkle Trie node/hash/update semantics.
- [ ] **M5-P5-S3** Apply VM state diffs atomically to an in-memory/test world state.
- [ ] **M5-P5-S4** Differential-test state roots and generated outputs.

## M5-P6 — Hard-fork VM parity

- [ ] **M5-P6-S1** Implement Mainnet/Leman/Rhone/Danube behavior through centralized fork rules.
- [ ] **M5-P6-S2** Add activation-boundary execution fixtures.
- [ ] **M5-P6-S3** Prove identical result, error, gas, output, and state behavior across boundaries.

## M5-P7 — M5 exit gate

- [ ] **M5-P7-S1** Stateless and stateful VM fixtures match Scala.
- [ ] **M5-P7-S2** Gas and failure behavior match Scala.
- [ ] **M5-P7-S3** Logical state diffs and roots match Scala.
- [ ] **M5-P7-S4** Global and differential VM gates pass with evidence.

---

# M6 — Persistent storage, recovery, and full synchronization

**Milestone status:** `[ ] BLOCKED BY M5`

Use RocksDB-compatible behavior first. Storage optimization or replacement belongs to M8.

## M6-P0 — Storage contract and official mapping

- [ ] **M6-P0-S1** Map official RocksDB sources, column families, key/value serializers, batches, indexes, and lifecycle.
- [ ] **M6-P0-S2** Define logical Rust storage traits for headers, blocks, transactions, world state, trie nodes, node/block state, mempool, broker data, logs, and optional indexes.
- [ ] **M6-P0-S3** Document atomicity, durability, pruning, migration, and recovery invariants.
- [ ] **M6-P0-S4** Add in-memory contract tests shared by all storage backends.

## M6-P1 — RocksDB backend

- [ ] **M6-P1-S1** Implement database open/close, column families, options, typed keys/values, and bounded iterators.
- [ ] **M6-P1-S2** Implement atomic cross-column write batches and snapshots.
- [ ] **M6-P1-S3** Implement block/header/transaction/state/trie/mempool/broker/log stores.
- [ ] **M6-P1-S4** Implement optional official indexes required by selected node scope.
- [ ] **M6-P1-S5** Add corruption/error propagation and no-panic tests.

## M6-P2 — Atomic commit and crash recovery

- [ ] **M6-P2-S1** Define block/state commit phases and durable commit markers.
- [ ] **M6-P2-S2** Implement restart recovery for interrupted writes.
- [ ] **M6-P2-S3** Add fault injection at every commit boundary.
- [ ] **M6-P2-S4** Verify reconstructed tips/state roots after simulated crashes.

## M6-P3 — Genesis and database initialization

- [ ] **M6-P3-S1** Reproduce network-specific genesis blocks/state.
- [ ] **M6-P3-S2** Persist initialization/version/bootstrap metadata.
- [ ] **M6-P3-S3** Verify Rust genesis hashes, roots, and metadata against official node.

## M6-P4 — Full sync scheduler

- [ ] **M6-P4-S1** Replace the M2 probe with bounded inventory/header/block/transaction synchronization pipelines.
- [ ] **M6-P4-S2** Validate before commit; never commit decode-only data.
- [ ] **M6-P4-S3** Implement dependency/orphan queues, retries, peer rotation, backpressure, and cancellation.
- [ ] **M6-P4-S4** Persist progress and resume after restart.

## M6-P5 — Reorg and state recovery

- [ ] **M6-P5-S1** Persist competing branches and canonical metadata.
- [ ] **M6-P5-S2** Apply official BlockFlow reorg decisions atomically.
- [ ] **M6-P5-S3** Restore mempool and indexes consistently after reorg/restart.
- [ ] **M6-P5-S4** Add deterministic reorg/recovery integration tests.

## M6-P6 — Historical replay and parity

- [ ] **M6-P6-S1** Replay official history from genesis on devnet/testnet fixture sets.
- [ ] **M6-P6-S2** Compare accepted blocks, chain tips, state roots, and indexed outputs with Scala.
- [ ] **M6-P6-S3** Expand toward full target-network replay.
- [ ] **M6-P6-S4** Record the first divergence with reproducible minimal evidence; never skip it.

## M6-P7 — M6 exit gate

- [ ] **M6-P7-S1** Rust syncs from genesis and reaches the same tips/state as the official node.
- [ ] **M6-P7-S2** Restart, interrupted-sync recovery, and reorg tests pass.
- [ ] **M6-P7-S3** Persistent database and version handling are documented.
- [ ] **M6-P7-S4** Historical replay evidence is recorded.

---

# M7 — Required node API, operations, packaging, and parity closure

**Milestone status:** `[ ] BLOCKED BY M6`

Do not let wallet/compiler parity delay a correct full node.

## M7-P0 — Scope freeze

- [ ] **M7-P0-S1** Enumerate official API/CLI/config behavior required for a production-compatible node.
- [ ] **M7-P0-S2** Separate required node scope from optional wallet, compiler, and developer tooling.
- [ ] **M7-P0-S3** Define endpoint/command/config parity fixtures.

## M7-P1 — Configuration and CLI

- [ ] **M7-P1-S1** Implement network/home/database/P2P/sync/index/log configuration.
- [ ] **M7-P1-S2** Implement safe identity, secrets, permissions, and startup validation.
- [ ] **M7-P1-S3** Implement status/version/shutdown/recovery CLI behavior.
- [ ] **M7-P1-S4** Differential-test required config precedence and errors.

## M7-P2 — HTTP/RPC/WebSocket API

- [ ] **M7-P2-S1** Implement minimum required read endpoints over validated canonical state.
- [ ] **M7-P2-S2** Implement transaction submission through full validation/mempool flow.
- [ ] **M7-P2-S3** Implement required subscriptions and backpressure.
- [ ] **M7-P2-S4** Match official schemas, errors, limits, and auth/CORS behavior for selected scope.

## M7-P3 — Observability and operations

- [ ] **M7-P3-S1** Add structured logs, peer/sync/storage/validation/VM metrics, health, and readiness.
- [ ] **M7-P3-S2** Add graceful shutdown and bounded drain behavior.
- [ ] **M7-P3-S3** Add backup/restore, database inspection, and troubleshooting runbooks.
- [ ] **M7-P3-S4** Add long-running soak and resource-leak tests.

## M7-P4 — Packaging and release

- [ ] **M7-P4-S1** Produce reproducible release binaries and checksums.
- [ ] **M7-P4-S2** Add container/systemd examples without weakening filesystem or key security.
- [ ] **M7-P4-S3** Define supported platforms, migration policy, and compatibility matrix.
- [ ] **M7-P4-S4** Complete license/attribution review.

## M7-P5 — Full compatibility exit gate

- [ ] **M7-P5-S1** Direct official peer interoperability passes.
- [ ] **M7-P5-S2** Genesis-to-tip sync and state parity pass.
- [ ] **M7-P5-S3** Validation, VM, gas, storage recovery, reorg, and required API parity pass.
- [ ] **M7-P5-S4** Independent review finds no unresolved consensus divergence.
- [ ] **M7-P5-S5** Project may now be described as a compatibility full node, with explicit supported scope.

---

# M8 — Measured implementation optimization

**Milestone status:** `[ ] BLOCKED BY M7`

No M8 change may alter official protocol behavior on the compatibility branch.

## M8-P0 — Baseline and profiling

- [ ] **M8-P0-S1** Freeze equivalent-hardware Scala and Rust benchmark environments.
- [ ] **M8-P0-S2** Measure sync throughput, tip latency, CPU, RAM, disk bytes, write amplification, network, and p50/p95/p99 latency.
- [ ] **M8-P0-S3** Profile before proposing optimization.
- [ ] **M8-P0-S4** Define regression thresholds with all parity suites mandatory.

## M8-P1 — Codec and networking optimization

- [ ] **M8-P1-S1** Reduce copies/allocations only where profiles justify it.
- [ ] **M8-P1-S2** Improve bounded buffering, batching, socket handling, and duplicate suppression.
- [ ] **M8-P1-S3** Preserve byte-for-byte and failure-behavior parity.

## M8-P2 — Validation and VM optimization

- [ ] **M8-P2-S1** Optimize hashing/signature verification/cache behavior based on profiles.
- [ ] **M8-P2-S2** Add deterministic parallel validation only when merge/order semantics are formally fixed.
- [ ] **M8-P2-S3** Optimize VM dispatch/state access without changing result, error, or gas.

## M8-P3 — Storage optimization

- [ ] **M8-P3-S1** Tune RocksDB caches, bloom filters, compaction, WAL, batches, and column-family isolation.
- [ ] **M8-P3-S2** Benchmark an append-only segment store for immutable block/transaction bodies behind existing traits.
- [ ] **M8-P3-S3** Benchmark alternative state backends only after RocksDB baseline and crash tests exist.
- [ ] **M8-P3-S4** Keep RocksDB as default until another backend wins representative replay, durability, recovery, and operational tests.

## M8-P4 — Optimization exit gate

- [ ] **M8-P4-S1** Publish reproducible before/after results.
- [ ] **M8-P4-S2** All wire, differential, VM, replay, recovery, and interoperability suites remain green.
- [ ] **M8-P4-S3** Remove optimizations that do not produce a material measured benefit.

---

# Deferred protocol research — not part of compatibility rebuild

The following are explicitly excluded from M0–M8 compatibility work:

- service-node fee rewards;
- 1M/100k/10k ALPH node tiers;
- new staking, voting, or node-selection roles;
- changed fee burning/distribution;
- increased groups/shards, gas limits, or transaction limits;
- new access lists or parallel-execution semantics;
- consensus or BlockFlow redesign;
- protocol-level compact blocks or new wire messages.

Create separate research proposals/branches only after M7 compatibility closure. Do not insert them into an active compatibility subphase.

## End-of-session mandatory update

Before ending a Codex session:

- update this file's checkbox and execution pointer;
- update detailed acceptance/evidence in `TASKS.md`;
- append exact commands/results/failures to `docs/WORKLOG.md`;
- update `docs/AI_HANDOFF.md` with one exact next subphase;
- update `docs/UPSTREAM_MAPPING.md` when official ownership coverage changes;
- include the active subphase ID in the commit message;
- leave no false claim that unexecuted tests passed.
