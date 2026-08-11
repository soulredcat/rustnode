# AI Session Prompt

Use this prompt when assigning repository work to a coding AI. Replace only the optional final scope line when necessary.

```text
You are working on the private repository soulredcat/rustnode.

Mission:
Build a Rust implementation of the official Alephium node that is protocol- and consensus-compatible with the existing Scala implementation. Compatibility comes before optimization. The immediate product goal is a working Rust node that can communicate directly with the pinned official node.

Official compatibility baseline:
https://github.com/alephium/alephium
commit 63bcf6998508025f9538f3377629fe9e66df6c53

Before changing anything, read these files in order:
1. README.md
2. AGENTS.md
3. TASKS.md
4. ARCHITECTURE.md
5. docs/UPSTREAM_MAPPING.md
6. the milestone document referenced by the current task
7. docs/WORKLOG.md

Execution rules:
- TASKS.md is authoritative. Select the highest-priority open/in-progress task whose dependencies are complete.
- Do not start a later milestone because it looks easier or more interesting.
- Inspect the exact pinned upstream source files and tests for the selected task.
- Never guess wire format, integer encoding, byte order, checksum, hashing, signature, VM, gas, hard-fork, or BlockFlow behavior.
- Create or use Scala-derived vectors/differential evidence before implementing wire- or consensus-critical behavior.
- Do not redesign or optimize the protocol during compatibility work.
- Do not add service-node economics, TPS changes, parallel VM changes, or new consensus rules.
- Do not use generic bincode/JSON/protobuf/default serde for official Alephium wire bytes.
- Do not panic on untrusted input. Bound all peer-controlled lengths, buffers, queues, and work.
- Keep deterministic protocol logic separate from async I/O orchestration.
- Keep the repository runnable and honest about what is not implemented.
- Do not describe decoded data as validated, or a handshake client as a full node.

Required workflow:
1. State the task ID and exact acceptance criteria you are implementing.
2. Record exact upstream files/tests consulted at the pinned commit.
3. Implement the narrowest end-to-end compatible slice.
4. Add tests and provenance-backed evidence.
5. Run make check plus the task-specific commands in TASKS.md.
6. Fix failures; do not mark the task done when commands were not run.
7. Update TASKS.md, docs/UPSTREAM_MAPPING.md when relevant, and append docs/WORKLOG.md.
8. End with changed files, commands/results, known uncertainty, and the exact next task.

When repository access or a required runtime is unavailable, do all independent work that remains possible, leave the task open, and record the exact blocker. Do not fabricate command output or compatibility evidence.

Optional requested scope for this session:
<leave empty to follow TASKS.md, or insert one existing task ID>
```

## Good scope examples

```text
M1-002 only. Implement the compact signed/unsigned integer primitives against committed Scala vectors. Do not implement the full message envelope.
```

```text
M1-007 only. Implement the TCP handshake state machine after M1-003 through M1-006 are marked DONE.
```

## Bad scope examples

```text
Make Alephium faster.
```

```text
Rewrite the entire official repository in one pass.
```

```text
Implement a Rust node based on general blockchain knowledge.
```

The task ledger exists specifically to prevent these ambiguous, unsafe assignments.
