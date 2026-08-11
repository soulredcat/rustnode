# AI Handoff

## Purpose

This document is the persistent state passed between coding sessions and AI agents. It must describe the repository as it actually exists, not what a previous chat intended to do.

Read this file after `TASKS.md` and verify its claims against the repository and test results before continuing.

## Current state

```text
Active milestone: M2 — Request and decode real network data
Active task:      M2 — Request and decode real network data
Task status:      IN PROGRESS (M2 assertion coverage hardened; official M2 CI path enabled)
Working branch:   main
Pinned upstream:  alephium/alephium@63bcf6998508025f9538f3377629fe9e66df6c53
```

### Completed repository work

- `README.md` defines compatibility-first and working-first goals.
- `ARCHITECTURE.md` defines the vertical milestone sequence.
- `AGENTS.md` defines repository-wide execution rules for humans and AI.
- `TASKS.md` defines task dependencies and acceptance gates.
- `docs/UPSTREAM_MAPPING.md` maps official modules and M1 source areas.
- `docs/INTEROPERABILITY.md` defines official-node test evidence.
- this handoff file and `docs/WORKLOG.md` provide persistent session state.

### Implementation status

```text
Cargo workspace:                 created
Rust crates:                     created and connected
Runnable Rust binary:            implemented
Official fixture generator:      committed and used
Message envelope:                implemented
Hello/Ping/Pong:                 implemented
Official-node interoperability:  direct official-node interop implemented and runtime-validated on Linux/WSL
```

Do not claim protocol milestones without local evidence attached in `docs/WORKLOG.md`.

## Next exact task

Complete M2 acceptance evidence:

- Add or run official-node interoperability evidence with M2 markers enabled;
- confirm request/response probe and ChainState behavior is stable over the soak period;
- optionally run with `RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS=1` after announcement behavior is consistently reproducible;
- document command/output evidence in `docs/WORKLOG.md` with concrete observed markers and exit status.

Latest local execution state:

- `cargo test -p alephium-flow --test rust_peer_smoke` now passes (4/4) after locator/fallback robustness fix in mock peer.
- `make official-interop-m2` could not be executed in this shell (`make`/`sbt` unavailable). Continue on WSL/Linux runner with the CI workflow or local WSL.

## Known risks

- The official serialization framework is custom; generic Rust serialization libraries must not define consensus/wire encoding.
- The exact message checksum preimage/algorithm still must be completed in M1-001.
- Exact network magic values and configuration sources must be mapped, not guessed.
- Broker/clique identity types and their cryptographic encoding must be mapped before Hello implementation.
- A Rust client ID must be proven acceptable to the official node; string similarity is not evidence.
- Licensing policy for mechanically ported/copied upstream source still requires explicit handling. Reimplementation should use behavior, specifications, and fixtures unless license obligations are intentionally adopted.

## Session-start checklist

An agent continuing work must:

- [ ] read `README.md`;
- [ ] read `ARCHITECTURE.md`;
- [ ] read `AGENTS.md`;
- [ ] read `TASKS.md`;
- [ ] read relevant files under `docs/`;
- [ ] inspect `git status` and recent commits;
- [ ] verify the active task is still unblocked;
- [ ] inspect upstream code required for that task;
- [ ] avoid repeating completed work.

## Session-end checklist

Before handing off:

- [ ] run required format/lint/test commands;
- [ ] record commands and actual results;
- [ ] update active task/status;
- [ ] list files changed;
- [ ] list upstream files/symbols inspected;
- [ ] record behavior implemented;
- [ ] record known gaps and failed attempts;
- [ ] identify one exact next task;
- [ ] append `docs/WORKLOG.md`;
- [ ] update the Current state section below.

## Handoff update template

Replace the Current state and Next exact task sections with factual state, then append details using this format:

```text
Active milestone:
Active task:
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

Tests added:
- test name -> behavior

Known gaps:
- ...

Decisions made:
- ADR/path or "none"

Next exact task:
- task ID
- exact first file/type/test to implement
```

## Rules for updating this file

- Keep only the latest active state in the main Current state section.
- Preserve historical detail in `docs/WORKLOG.md`.
- Never mark a task `DONE` when an external acceptance gate was not executed.
- Never write "all tests pass" without listing the command used.
- Never hide a failing test to make the handoff look clean.
- Do not use this file as a speculative roadmap; use `TASKS.md` for planned work.
