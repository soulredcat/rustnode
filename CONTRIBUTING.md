# Contributing

## Scope

This project rebuilds the official Alephium node in Rust. Compatibility comes before optimization or protocol research.

Read `AGENTS.md` and `TASKS.md` before opening a change.

## Select a task

Every change should map to one task ID from `TASKS.md`.

Do not combine unrelated upstream modules or milestones into one change. When a task is too broad, split it in the ledger first.

## Before implementation

Document in the PR or commit description:

- task ID;
- pinned upstream files and tests consulted;
- observable behavior being reproduced;
- required evidence/vector source;
- explicit out-of-scope behavior.

## Code requirements

- preserve one-way crate dependencies;
- keep wire/consensus code deterministic;
- do not panic on untrusted input;
- bound all peer-controlled lengths and work;
- do not use floating-point arithmetic in protocol-critical calculations;
- do not introduce `unsafe` without a separately reviewed architecture decision;
- do not use generic serialization for official Alephium bytes;
- do not alter protocol behavior to make the Rust implementation easier.

## Verification

Run:

```bash
make check
```

Then run the task-specific command from `TASKS.md`.

A wire/consensus change normally also requires one of:

- Scala-derived golden vector;
- Rust/Scala interoperability test;
- differential validation/VM test;
- historical replay assertion.

## Required documentation updates

When completing a task, update:

- `TASKS.md`;
- `docs/UPSTREAM_MAPPING.md` when applicable;
- `docs/WORKLOG.md`.

## Commit guidance

Preferred format:

```text
<area>: <imperative summary> [TASK-ID]
```

Examples:

```text
workspace: add minimum crate graph [M0-001]
codec: decode official message envelope [M1-003]
p2p: complete Scala Hello handshake [M1-009]
```

## Pull request body

Include:

```text
Task:
Upstream sources:
Behavior reproduced:
Tests/commands:
Evidence:
Known limitations:
Next task:
```

Do not describe an implementation as interoperable, validated, or full-node-ready without the corresponding evidence defined in `AGENTS.md`.
