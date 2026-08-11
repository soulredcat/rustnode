# Documentation Index

Use this directory together with the root project documents. Repository state, not prior chat history, is authoritative.

## Start here

Read in this order:

1. [`../README.md`](../README.md) — project mission and first working target.
2. [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — system boundaries and vertical milestones.
3. [`../AGENTS.md`](../AGENTS.md) — mandatory execution rules for humans and AI agents.
4. [`../TASKS.md`](../TASKS.md) — active task queue, dependencies, and acceptance gates.
5. [`UPSTREAM_MAPPING.md`](./UPSTREAM_MAPPING.md) — mapping from official Scala modules/symbols to Rust work.
6. [`INTEROPERABILITY.md`](./INTEROPERABILITY.md) — direct official-node test specification.
7. [`AI_HANDOFF.md`](./AI_HANDOFF.md) — latest repository/session state and next exact task.
8. [`WORKLOG.md`](./WORKLOG.md) — append-only implementation history.
9. [`decisions/`](./decisions/) — accepted architecture decision records.

## Document responsibilities

| Document | What it answers |
|---|---|
| `README.md` | What are we building, and what is the first useful result? |
| `ARCHITECTURE.md` | How are modules separated, and in what order is the node built? |
| `AGENTS.md` | What must every contributor/AI do and avoid? |
| `TASKS.md` | What is the next executable task and how is it proven complete? |
| `UPSTREAM_MAPPING.md` | Which official files/symbols define each Rust behavior? |
| `INTEROPERABILITY.md` | How do we prove the Rust node works with an unmodified official node? |
| `AI_HANDOFF.md` | What is the exact current state and next action? |
| `WORKLOG.md` | What was actually changed and tested in previous sessions? |
| `decisions/*.md` | Why was an architectural or protocol-development decision accepted? |

## Update policy

- Update `TASKS.md` whenever scope, dependencies, or task status changes.
- Append `WORKLOG.md` at the end of every implementation session.
- Replace the current-state section in `AI_HANDOFF.md` at handoff.
- Update `UPSTREAM_MAPPING.md` whenever a new official symbol is implemented.
- Update `INTEROPERABILITY.md` whenever the official test harness or evidence changes.
- Add an ADR for durable decisions that affect module boundaries, protocol interpretation, persistence format, cryptographic dependencies, async model, or compatibility policy.
- Do not put speculative implementation status in `README.md`.

## Current entry point

The current active task is defined in [`../TASKS.md`](../TASKS.md). At the time this index was created, it is:

```text
M0-001 — Create the minimal Cargo workspace
```

Verify that this remains current before starting work.