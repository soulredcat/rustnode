# Execution Ledger

Last updated: `2026-08-11`

## Status legend

- `DONE` — acceptance criteria passed and evidence recorded.
- `IN PROGRESS` — one active owner/agent should finish or hand off.
- `OPEN` — ready after dependencies are complete.
- `BLOCKED` — cannot proceed; reason and unblock condition required.

## Current position

```text
Current milestone: M1 — official P2P handshake and Ping/Pong
Current active gate: M1-005 — handshake persistence and identity lifecycle
Next implementation task: M1-002 — bounded Alephium binary codec primitives
```

M0 is complete. No task beyond M1 should begin until the official Scala node accepts the Rust handshake.

---

## M0 — Repository bootstrap

**Milestone status:** `DONE`

### M0-001 — Establish project control documents and minimum Cargo workspace

**Status:** `DONE`

**Scope:**

- compatibility-first README and architecture;
- root `AGENTS.md` for human/AI execution;
- milestone/task ledger;
- upstream mapping and M1 runbook;
- pinned upstream fetch script;
- minimum compiling crate graph;
- formatting/lint/test command;
- CI workflow.

**Evidence:** commit `ea22de1e5c3557c65d57910329de1619dc7aac51`.

### M0-002 — Prove bootstrap workspace is green

**Status:** `DONE`

**Commands verified:**

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

**Evidence:**

- fix commit: `be4dbc86aa31cd26af65712697df6ca269c01fcd`;
- successful GitHub Actions run: `31459463053`;
- CI toolchain: `rustc 1.97.1 (8bab26f4f 2026-07-14)`.

### M0-003 — Verify pinned upstream acquisition

**Status:** `DONE`

**Command:**

```bash
./scripts/fetch-upstream.sh
```

**Evidence:**

- CI integration commit: `940e58f00e2c1f9c43f5c2e51002cfc12a1e518c`;
- successful GitHub Actions run: `31459708994`;
- verified HEAD: `63bcf6998508025f9538f3377629fe9e66df6c53`.

### M0-004 — Create a reproducible local official-node interoperability environment

**Status:** `DONE`

**Delivered:**

- isolated devnet configuration in `config/official-node/user.conf`;
- pinned official-node build/run/smoke scripts;
- loopback-only P2P endpoint `127.0.0.1:19973`;
- no bootstrap peers and no public-network malformed traffic;
- reproducible runbook in `docs/LOCAL_OFFICIAL_NODE.md`;
- dedicated GitHub Actions smoke workflow.

**Evidence:**

- harness commit: `c168d2bfef5d1a7d624f54697dedd18dd1006ce3`;
- workflow-order fix: `88796ffae76abe76abf69d7f38936ffd7aa71da7`;
- successful GitHub Actions run: `31460207593`;
- official JAR built from the pinned commit and opened `127.0.0.1:19973`.

### M0 exit gate

**Passed.** All M0 tasks are `DONE`.

---

## M1 — Official P2P handshake and Ping/Pong

Read these before implementation:

1. `docs/M1_P2P_INTEROP.md`;
2. `docs/M1_WIRE_REFERENCE.md`;
3. `docs/UPSTREAM_MAPPING.md`;
4. `tests/vectors/m1/README.md`.

### M1-001 — Freeze official M1 behavior and create Scala-derived vectors

**Status:** `DONE`

**Upstream minimum:**

- `protocol/.../message/Message.scala`;
- `protocol/.../message/MessageSerde.scala`;
- `protocol/.../message/Header.scala`;
- `protocol/.../message/Payload.scala`;
- `protocol/.../message/P2PVersion.scala`;
- `protocol/.../message/RequestId.scala`;
- `protocol/.../model/ReleaseVersion.scala`;
- `protocol/.../model/BrokerInfo.scala`;
- `protocol/.../model/CliqueInfo.scala`;
- `protocol/.../model/CliqueId.scala`;
- `serde/.../CompactInteger.scala` and its tests;
- official SecP256K1 implementation;
- relevant protocol and flow tests listed in `docs/UPSTREAM_MAPPING.md`.

**Delivered so far:**

- source-derived wire reference;
- upstream mapping;
- reproducible Scala generator source;
- generator CI workflow and artifact upload path;
- deterministic devnet identity/message fixture design;
- valid, malformed, and truncation fixture coverage defined.

**Completion evidence:** `tests/vectors/m1/generated` now contains canonical artifacts (`manifest.tsv`, `hello-frame.hex`, `invalid/` fixtures, compact integer tables, checksums), and vector provenance is documented in `tests/vectors/m1/README.md` with reviewed workflow run references.

### M1-002 — Implement bounded Alephium binary codec primitives required by M1

**Status:** `DONE`

**Dependency:** M1-001 `DONE`.

**Scope:** only fixed big-endian values, compact signed/unsigned integers, fixed bytes, bounded strings, vectors/products, and staging behavior actually required by M1.

**Acceptance:** all M1 primitive vectors pass; malformed, truncated, overflow, and allocation-limit cases fail without panic.

### M1-003 — Implement official message envelope codec

**Status:** `DONE`

**Dependency:** M1-002.

**Scope:** network magic, DJB checksum, fixed four-byte outer length, message header, payload code, and incremental TCP decode state.

**Acceptance:** byte-for-byte parity with Scala vectors; supports fragmented TCP reads and multiple frames in one buffer; rejects wrong magic/checksum/length according to upstream behavior.

### M1-004 — Implement M1 protocol models

**Status:** `DONE`

**Dependency:** M1-002.

**Scope:** timestamp, request ID, wire header/version, P2P version, release/client ID parsing, broker/clique structures required by `Hello`, `Ping`, and `Pong`.

**Acceptance:** canonical encode/decode parity and upstream validation vectors pass.

### M1-005 — Implement official handshake cryptography and identity persistence

**Status:** `IN PROGRESS`

**Dependency:** M1-001 and M1-004.

**Scope:** official SecP256K1 formats/signing/verification required by handshake; persistent identity file with atomic creation and restrictive permissions where supported.

**Current state:** signing and validation now behave against official fixtures; identity persistence is still in-memory/config fixture and is not yet persisted across restarts in production-safe storage.

**Acceptance:** Rust verifies Scala signatures and Scala verifies Rust-produced `Hello` signatures.

### M1-006 — Implement `Hello`, `Ping`, and `Pong`

**Status:** `DONE`

**Dependency:** M1-003, M1-004, and M1-005.

**Acceptance:** payload bytes match Scala; invalid signature/timestamp/broker info are handled consistently; client ID is parser-compatible and honestly identifies the Rust implementation.

### M1-007 — Implement TCP connection and handshake state machine

**Status:** `DONE`

**Dependency:** M1-003, M1-006, and M0-004.

**State model:**

```text
Disconnected -> Connecting -> TcpConnected -> HelloSent
-> Established -> Closing -> Disconnected
```

**Acceptance:** timeouts, EOF, malformed frame, invalid `Hello`, and clean shutdown do not panic or leak an unbounded task/queue.

### M1-008 — Implement Ping/Pong liveness

**Status:** `DONE`

**Dependency:** M1-007.

**Acceptance:** request IDs are correlated according to official semantics; timeout behavior is documented and tested.

### M1-009 — Official Scala interoperability gate

**Status:** `DONE`

**Dependency:** M1-008.

**Required evidence:**

- pinned official node logs showing Rust peer accepted;
- Rust logs showing valid official `Hello` received;
- successful Ping/Pong in the required directions;
- connection remains established for 120s soak interval;
- no adapter/proxy translating messages (`make official-interop-m1` direct peer session);
- commands/configs logged to `tests/interoperability/m1/commands.txt` with `official-start-command`, `rust-start-command`, and commit SHA.

### M1 exit gate

M1-005 remains open for persisted identity lifecycle. Other gates are currently `DONE`.

---

## M2 — Request and decode real network data

**Status:** `BLOCKED` by M1.

Planned sequence:

1. chain state/inventory;
2. header request/response;
3. block request/response;
4. new header/block announcements;
5. transaction inventory/request/response.

Exit condition: Rust requests and decodes real headers and blocks from the official node while clearly labeling them `decoded`, not yet `validated`.

---

## M3 — Protocol model and canonical hash parity

**Status:** `BLOCKED` by M2.

Exit condition: Scala and Rust produce identical canonical bytes, transaction IDs, header hashes, block hashes, and protocol-visible cryptographic results for supported data.

---

## M4 — Validation and BlockFlow parity

**Status:** `BLOCKED` by M3.

Exit condition: Rust accepts/rejects the same transaction, header, and block fixtures and derives the same BlockFlow decisions.

---

## M5 — Ralph/Alphred VM and state parity

**Status:** `BLOCKED` by M4.

Exit condition: result values, errors, gas, outputs, contract state, and hard-fork behavior match Scala.

---

## M6 — Persistent full synchronization

**Status:** `BLOCKED` by M5.

Exit condition: Rust syncs from genesis, persists/restarts safely, and reaches the same valid chain tips/state as the official node.

---

## M7 — Required API and tooling parity

**Status:** `BLOCKED` by M6.

Implement only APIs/tools required by the agreed node scope. Wallet/compiler parity is not allowed to delay full-node correctness.

---

## M8 — Optimization

**Status:** `BLOCKED` by M7.

No optimization claim is accepted without equivalent-hardware benchmarks and all parity/replay gates remaining green.
