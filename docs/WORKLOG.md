# Worklog and Agent Handoff

This file records completed work, failed attempts, commands, and the exact next action. Append entries; do not rewrite history except to correct an explicitly noted factual error.

## 2026-08-11 — Compatibility-first project definition

**Task:** documentation bootstrap before task IDs were introduced.

**Changed:**

- established the objective: Rust node must first work with the official Scala node;
- pinned official baseline commit `63bcf6998508025f9538f3377629fe9e66df6c53`;
- created compatibility-first `README.md`;
- created `ARCHITECTURE.md` with vertical milestone sequence;
- explicitly deferred optimization and protocol redesign.

**Upstream sources reviewed:**

- official repository metadata and `README.md`;
- `build.sbt` module graph;
- `Message.scala`;
- `MessageSerde.scala`;
- `Payload.scala`;
- `P2PVersion.scala`;
- `ReleaseVersion.scala`;
- `BrokerHandler.scala`;
- selected model/network file locations.

**Known limitations:** no Rust code, build, or live interoperability existed at this point.

## 2026-08-11 — M0-001 AI-operable scaffold

**Task:** `M0-001`.

**Implementation commit:** `ea22de1e5c3557c65d57910329de1619dc7aac51`.

**Changed:**

- added `AGENTS.md`, `TASKS.md`, contribution rules, upstream mapping, M1 runbook, and AI task template;
- added pinned upstream manifest/fetch script;
- added minimum Cargo workspace and status-only app binary;
- added local check script, Makefile, formatting configuration, and CI workflow;
- added test-layout documentation.

**Commands run:**

- Runtime compilation was not available in the authoring environment; Rust toolchain was not installed there.
- Therefore `M0-002` remained open until GitHub Actions verified the scaffold.

**Known uncertainty:** scaffold syntax was intentionally simple, but was not considered verified until the required Cargo commands passed.

**Repository note:** an intermediate README-only branch history was discarded while attaching the complete Git tree to `main`; the retained `main` content is the complete M0-001 tree above.

## 2026-08-11 — M0-002 green Rust workspace

**Task:** `M0-002`.

**Fix commit:** `be4dbc86aa31cd26af65712697df6ca269c01fcd`.

**GitHub Actions:** run `31459463053`, conclusion `success`.

**Toolchain:**

```text
rustc 1.97.1 (8bab26f4f 2026-07-14)
```

**Commands verified by CI:**

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

**Failure handled before success:** the first run detected a rustfmt-only difference in `crates/util/src/lib.rs`. The source was formatted and the checkout action was upgraded from v4 to v7 so the workflow uses the current Node 24 action runtime.

**Result:** minimum workspace compiles, lints, and tests successfully. This does not imply any Alephium protocol implementation exists.

## 2026-08-11 — M0-003 pinned upstream verified

**Task:** `M0-003`.

**CI integration commit:** `940e58f00e2c1f9c43f5c2e51002cfc12a1e518c`.

**GitHub Actions:** run `31459708994`, conclusion `success`.

**Command verified:**

```bash
./scripts/fetch-upstream.sh
test "$(git -C .upstream/alephium rev-parse HEAD)" = \
  "63bcf6998508025f9538f3377629fe9e66df6c53"
```

**Result:** CI now proves each build can acquire the exact official compatibility baseline. It does not silently build against upstream `master`.

**Exact next task:** `M0-004` — create and verify `docs/LOCAL_OFFICIAL_NODE.md`, including isolated configuration, startup readiness, P2P endpoint, shutdown, and cleanup.
