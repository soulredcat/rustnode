# AI Handoff

## Purpose

This document is the factual state passed between coding sessions and AI agents. It must describe the repository as it exists, not what a previous chat intended.

Read this file after `TODO.md` and `TASKS.md`, then verify every claim against the repository and test/workflow evidence before continuing.

## Current state

```text
Active milestone: M2 — Request and decode real network data
Active phase:     M2-P0 — Restore a green, reproducible M2 baseline
Active subphase:  M2-P0-S1 — Apply rustfmt and restore the local formatting gate
Task status:      ACTIVE
Working branch:   main
Pinned upstream:  alephium/alephium@63bcf6998508025f9538f3377629fe9e66df6c53
Latest protocol implementation commit before TODO integration:
                  e0c7d3e9c9f8659f1e54a37aeb7fc101d9ac5bb9
```

### Completed repository work

- `README.md` defines compatibility-first and working-first goals.
- `ARCHITECTURE.md` defines module boundaries and vertical milestone progression.
- `AGENTS.md` defines mandatory execution rules for humans and AI.
- `TODO.md` defines the ordered milestone/phase/subphase tree and current execution pointer.
- `TASKS.md` defines detailed acceptance gates and evidence.
- `docs/UPSTREAM_MAPPING.md` maps official source areas to Rust ownership.
- `docs/INTEROPERABILITY.md` defines official-node test evidence.
- this handoff and `docs/WORKLOG.md` preserve session state/history.

### Implementation status

```text
Cargo workspace:                  implemented
Rust crates:                      implemented and connected
Runnable Rust binary:             implemented
Pinned Scala fixture generator:   implemented and used for M1
Message envelope:                 implemented for current scope
Hello/Ping/Pong:                  implemented
Direct official M1 interop:       completed and previously runtime-validated
M2 sync payload handling:         partially implemented; verification/closure active
Canonical protocol validation:    not implemented
BlockFlow parity:                 not implemented
Ralph/Alphred VM parity:          not implemented
Persistent canonical storage:     not implemented
Genesis-to-tip full sync:         not implemented
Production full-node status:      not achieved
```

## Current blockers and evidence

### Rust CI formatting gate

The latest Rust CI run for commit `e0c7d3e9c9f8659f1e54a37aeb7fc101d9ac5bb9` stopped at:

```bash
cargo fmt --all -- --check
```

The reported diff is formatting-related across `crates/flow/src/lib.rs`, `crates/flow/tests/rust_peer_smoke.rs`, and `crates/protocol/src/message/payload.rs`. Compile, Clippy, and tests were skipped after that failure.

### Official interoperability workflow ordering

`.github/workflows/m1-official-interop.yml` invokes Java setup with cache paths under `.upstream/alephium` before executing `./scripts/fetch-upstream.sh`. The latest workflow therefore failed before reaching the M2 harness because the cache dependency paths did not exist.

These are the first two ordered M2-P0 repairs. Do not start M3.

## Next exact task

### `M2-P0-S1`

1. Run `cargo fmt --all`.
2. Inspect the diff and confirm it contains no semantic behavior changes.
3. Run:

```bash
cargo fmt --all -- --check
```

4. If it passes, mark `M2-P0-S1` complete in `TODO.md` and move the pointer to `M2-P0-S2`.
5. Update `TASKS.md`, append `docs/WORKLOG.md`, and update this handoff.
6. Use `M2-P0-S1` in the commit message.

Do not combine the workflow-order repair (`M2-P0-S3`) into this formatting subphase.

## Known risks

- The current M2 implementation contains substantial logic in a large `crates/flow/src/lib.rs`; modular separation is explicitly scheduled in `M2-P6-S1`, not during formatting repair.
- Some M2 payload handling is implemented but remains `[VERIFY]`; existing code must be tested against pinned official evidence rather than rewritten from assumptions.
- Official serialization is custom; generic Rust serialization must not define wire or consensus bytes.
- Current M2 means decode/request interoperability only, not validation or canonical state.
- Licensing/attribution policy for mechanically copied or translated upstream code still requires explicit closure before release.

## Session-start checklist

An agent continuing work must:

- [ ] read `README.md`;
- [ ] read `AGENTS.md`;
- [ ] read `TODO.md` and identify the exact active pointer;
- [ ] read `TASKS.md`;
- [ ] read `ARCHITECTURE.md`;
- [ ] read relevant files under `docs/`;
- [ ] inspect Git status, current HEAD, recent commits, and CI state;
- [ ] verify active subphase dependencies;
- [ ] inspect upstream code only when the active subphase changes protocol behavior;
- [ ] avoid repeating completed work or jumping milestones.

## Session-end checklist

Before handing off:

- [ ] run required format/check/Clippy/test commands for the active subphase;
- [ ] run required official/differential/replay/recovery gate when applicable;
- [ ] record commands and actual results;
- [ ] update the active TODO checkbox and execution pointer;
- [ ] update `TASKS.md` acceptance/evidence;
- [ ] list files changed;
- [ ] list upstream files/symbols inspected;
- [ ] record behavior implemented;
- [ ] record known gaps and failed attempts;
- [ ] identify one exact next subphase;
- [ ] append `docs/WORKLOG.md`;
- [ ] update this Current state section.

## Handoff update template

```text
Active milestone:
Active phase:
Active subphase:
Task status:
Working branch:
Latest relevant commit:

Files changed:
- ...

Upstream inspected:
- path :: symbol

Behavior implemented:
- ...

Commands executed:
- command
  result: PASS/FAIL/BLOCKED
  relevant output: ...

Evidence added:
- ...

Known gaps:
- ...

Decisions made:
- ADR/path or "none"

TODO pointer after session:
- ...

Next exact subphase:
- ID, title, first file/test/command
```

## Rules for updating this file

- Keep only the latest active state in the main Current state section.
- Preserve historical detail in `docs/WORKLOG.md`.
- Never mark a TODO checkbox or task `DONE` when a required acceptance gate was not executed.
- Never write "all tests pass" without listing the exact command.
- Never hide a failing test/workflow to make the handoff appear clean.
- Do not use this file as a speculative roadmap; use `TODO.md` for ordered work and `TASKS.md` for detailed gates.
