# Documentation Index

Use this directory together with the root project documents. Repository state, not prior chat history, is authoritative.

## Start here

Read in this order:

1. [`../README.md`](../README.md) — project mission and current status.
2. [`../AGENTS.md`](../AGENTS.md) — mandatory execution rules for humans and AI agents.
3. [`../TODO.md`](../TODO.md) — ordered milestone/phase/subphase plan and current execution pointer.
4. [`../TASKS.md`](../TASKS.md) — detailed acceptance criteria, status narrative, and evidence.
5. [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — system boundaries and vertical milestone architecture.
6. [`UPSTREAM_MAPPING.md`](./UPSTREAM_MAPPING.md) — mapping from official Scala modules/symbols to Rust work.
7. [`INTEROPERABILITY.md`](./INTEROPERABILITY.md) — direct official-node test specification.
8. [`AI_HANDOFF.md`](./AI_HANDOFF.md) — latest factual repository/session state and next exact subphase.
9. [`WORKLOG.md`](./WORKLOG.md) — append-only implementation history.
10. [`decisions/`](./decisions/) — accepted architecture decision records.

## Document responsibilities

| Document | What it answers |
|---|---|
| `README.md` | What are we building and what is currently implemented? |
| `AGENTS.md` | What must every contributor/AI do and avoid? |
| `TODO.md` | Which milestone, phase, and subphase must be executed next? |
| `TASKS.md` | What acceptance criteria and evidence prove the work complete? |
| `ARCHITECTURE.md` | How are modules separated and how does the full node fit together? |
| `UPSTREAM_MAPPING.md` | Which official files/symbols define each Rust behavior? |
| `INTEROPERABILITY.md` | How do we prove the Rust node works with an unmodified official node? |
| `AI_HANDOFF.md` | What is the exact current state and next action? |
| `WORKLOG.md` | What was actually changed and tested in previous sessions? |
| `decisions/*.md` | Why was an architectural or protocol-development decision accepted? |

## Authority split

- The pinned official Scala implementation is the behavioral source of truth.
- `TODO.md` is authoritative for execution order and the active subphase.
- `TASKS.md` is authoritative for acceptance criteria and evidence.
- `ARCHITECTURE.md` is authoritative for dependency direction and module ownership boundaries.

If documents disagree, fix the disagreement in the same change. Do not silently choose a convenient interpretation.

## Update policy

- Update `TODO.md` whenever milestone/phase/subphase order, dependency, checkbox status, or current pointer changes.
- Update `TASKS.md` whenever detailed scope, acceptance, blocker, or evidence changes.
- Append `WORKLOG.md` at the end of every implementation session.
- Replace the current-state section in `AI_HANDOFF.md` at handoff.
- Update `UPSTREAM_MAPPING.md` whenever a new official symbol is implemented or ownership changes.
- Update `INTEROPERABILITY.md` whenever the official test harness or evidence requirements change.
- Add an ADR for durable decisions that affect module boundaries, protocol interpretation, persistence format, cryptographic dependencies, async model, or compatibility policy.
- Do not put speculative implementation status in `README.md`.

## Current entry point

The exact current entry point is defined in [`../TODO.md`](../TODO.md). At this update it is:

```text
M2-P0-S1 — Apply rustfmt and restore the local formatting gate
```

Verify the pointer, dependencies, Git state, and current CI results before starting work. Do not select another subphase while this one remains active.
