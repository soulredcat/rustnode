# Reusable AI Build Prompt

Use [`docs/AI_SESSION_PROMPT.md`](./docs/AI_SESSION_PROMPT.md) as the canonical copy-paste prompt for Codex or another coding agent.

The shorter prompt below is provided for environments where the repository is already mounted and agent instructions are automatically loaded.

---

## Implementation prompt

```text
You are implementing the Alephium Rust compatibility node in soulredcat/rustnode.

Do not rely on previous chat history. Repository files and the pinned official Alephium source are authoritative.

Read in order:
1. README.md
2. AGENTS.md completely
3. TODO.md
4. TASKS.md
5. ARCHITECTURE.md
6. docs/UPSTREAM_MAPPING.md
7. the runbook referenced by the active TODO subphase
8. docs/AI_HANDOFF.md
9. the latest relevant docs/WORKLOG.md entry

Then inspect Git status, current HEAD, recent commits, existing tests, and CI state.

Authority:
- official Alephium Scala behavior is the compatibility oracle;
- TODO.md defines the mandatory milestone/phase/subphase order and current execution pointer;
- TASKS.md defines detailed acceptance criteria and evidence;
- work on exactly one active TODO subphase;
- do not jump ahead or combine later subphases.

Pinned upstream:
https://github.com/alephium/alephium
commit 63bcf6998508025f9538f3377629fe9e66df6c53

Mission:
Build a Rust node that is accepted by, synchronizes with, validates the same data as, executes the same VM/state transitions as, persists/restarts safely against, and derives the same state as the unmodified official Scala node.

Restrictions:
- no consensus, wire, transaction, block, hash, signature, VM, gas, hard-fork, group/shard, limit, or tokenomic changes;
- no service-node economics or fee redistribution;
- no guessed behavior;
- no generic serializer defining official bytes;
- no panics or unbounded resources from untrusted input;
- no optimization before parity and profiling;
- no success claim without required evidence.

Workflow:
1. State active subphase ID/title, dependencies, upstream sources, and acceptance gate.
2. Verify TODO prerequisites.
3. Inspect exact pinned upstream source/tests.
4. Define observable behavior and provenance-backed fixtures/evidence.
5. Implement only the active subphase.
6. Run task-specific gates plus:
   cargo fmt --all -- --check
   cargo check --workspace --all-targets
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace
7. Leave the checkbox open when any required external/differential/interoperability/replay/recovery gate was not executed.
8. On completion, update TODO.md checkbox and pointer, TASKS.md evidence, docs/UPSTREAM_MAPPING.md when relevant, docs/WORKLOG.md, and docs/AI_HANDOFF.md.
9. Commit coherent changes with the active subphase ID.

If blocked, prove and record the blocker. Do not switch milestones.

End report:
Subphase status:
Files changed:
Upstream symbols inspected:
Behavior implemented:
Commands/results:
Evidence:
Known gaps:
Updated TODO pointer:
Next exact subphase:
Commit SHA:
```

---

## Review-only prompt

```text
Audit the currently active TODO.md subphase in soulredcat/rustnode against:
- the pinned official Alephium commit;
- README.md;
- AGENTS.md;
- TODO.md;
- TASKS.md;
- ARCHITECTURE.md;
- docs/UPSTREAM_MAPPING.md;
- docs/INTEROPERABILITY.md;
- docs/AI_HANDOFF.md;
- relevant worklog evidence.

Do not review style first. Review correctness in this order:
1. byte/wire compatibility;
2. validation and failure behavior;
3. cryptographic preimages and encodings;
4. network/hard-fork configuration;
5. deterministic behavior;
6. bounded resource handling;
7. async cancellation/backpressure;
8. official fixtures/differential/interoperability/replay/recovery evidence;
9. subphase and milestone Definition of Done;
10. maintainability.

For every finding provide:
- severity;
- active subphase ID;
- Rust file/symbol;
- official upstream file/symbol;
- exact divergence or unproven assumption;
- minimal reproduction/test;
- required fix.

Do not approve a subphase merely because it compiles or has Rust-only round-trip tests. Confirm that the TODO pointer was not advanced prematurely.
```

---

## Usage note

The prompts are intentionally strict. A blockchain compatibility implementation must prefer a slower evidenced result over a fast implementation based on assumptions.
