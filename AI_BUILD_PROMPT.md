# Reusable AI Build Prompt

Copy the prompt below into a capable coding agent that has access to this repository.

---

## Implementation prompt

You are implementing the Alephium Rust compatibility node in this repository.

Do not rely on previous chat history. Treat repository files and the pinned official Alephium source as authoritative.

Before changing anything:

1. Read `README.md`.
2. Read `ARCHITECTURE.md`.
3. Read `AGENTS.md` completely and follow it as mandatory repository policy.
4. Read `TASKS.md` and select only the first unblocked `READY` task in the active milestone.
5. Read `docs/README.md`, `docs/UPSTREAM_MAPPING.md`, `docs/AI_HANDOFF.md`, and the latest `docs/WORKLOG.md` entry.
6. Read `docs/INTEROPERABILITY.md` for any networking, codec, identity, version, or P2P work.
7. Inspect the current repository tree, Git status, recent commits, and existing tests before writing code.
8. Inspect every relevant official upstream file/symbol at the pinned commit before implementing protocol behavior.

Project mission:

> Build a working Rust implementation that is accepted by, synchronizes with, validates the same data as, and derives the same state as the unmodified official Alephium Scala node.

Pinned upstream:

```text
https://github.com/alephium/alephium
commit 63bcf6998508025f9538f3377629fe9e66df6c53
```

Non-negotiable restrictions during V1:

- no consensus changes;
- no transaction/block/wire-format changes;
- no fee redistribution or service-node implementation;
- no new staking or voting role;
- no changed Ralph/Alphred VM semantics;
- no changed gas or hard-fork behavior;
- no guessed constants, field order, byte order, checksum, hash preimage, signature format, timeout, or network configuration;
- no optimization that lacks parity evidence;
- no unbounded allocation or queue reachable from network input;
- no panics on untrusted input;
- no claim of success without running the required tests.

Execution requirements:

1. Work on one task ID at a time.
2. Write down exact upstream paths and symbols that define the behavior.
3. Define observable compatibility requirements before implementation.
4. Add official golden vectors, differential tests, or direct official-node interoperability evidence as required by the task.
5. Implement the smallest complete vertical behavior.
6. Keep deterministic codec/validation/VM/state logic synchronous; use async only at I/O orchestration boundaries.
7. Use typed errors and fail closed.
8. Keep all input lengths, buffers, channels, retries, and concurrency bounded.
9. Run the task's exact acceptance commands plus:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

10. Do not mark the task `DONE` if an external fixture or official-node test was not actually executed.
11. Update `TASKS.md`, append `docs/WORKLOG.md`, and update `docs/AI_HANDOFF.md` before ending.
12. Commit only coherent, reviewable changes and include the task ID in the commit message.

At the start of your response, state only:

```text
Active task: <task ID and title>
Upstream files to inspect: <paths>
Acceptance gate: <exact tests/result>
```

Then inspect and implement. Do not ask for confirmation when the repository already defines the answer. If blocked, prove the blocker with exact evidence, record it in the task/handoff files, and complete every independent part that can still be completed safely.

At the end, report:

```text
Task status:
Files changed:
Upstream symbols inspected:
Behavior implemented:
Commands executed and exact result:
Parity/interoperability evidence:
Known gaps:
Next exact task:
Commit SHA:
```

---

## Review-only prompt

Use this when asking another AI to audit an implementation without modifying it:

```text
Audit the currently active task in this repository against README.md, ARCHITECTURE.md, AGENTS.md, TASKS.md, docs/UPSTREAM_MAPPING.md, docs/INTEROPERABILITY.md, and the pinned official Alephium commit.

Do not review style first. Review correctness in this order:
1. byte/wire compatibility;
2. validation and failure behavior;
3. cryptographic preimages and encodings;
4. network/hard-fork configuration;
5. deterministic behavior;
6. bounded resource handling;
7. async cancellation/backpressure;
8. official fixtures/differential/interoperability evidence;
9. tests and task Definition of Done;
10. maintainability.

For every finding, provide:
- severity;
- Rust file/symbol;
- official upstream file/symbol;
- exact divergence or unproven assumption;
- minimal reproduction/test;
- required fix.

Do not approve a task merely because it compiles or has Rust-only round-trip tests.
```

---

## Usage note

The prompt is intentionally strict. A blockchain compatibility implementation must prefer a slower, evidenced result over a fast implementation based on assumptions.