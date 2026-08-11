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

## 2026-08-11 — M0-004 official-node harness verified

**Task:** `M0-004`.

**Harness commit:** `c168d2bfef5d1a7d624f54697dedd18dd1006ce3`.

**Workflow-order fix:** `88796ffae76abe76abf69d7f38936ffd7aa71da7`.

**GitHub Actions:** run `31460207593`, conclusion `success`.

**Environment and evidence:**

```text
OS: Ubuntu 24.04
Java: Temurin 17.0.19
Official sbt build: 1.11.5
Official source: 63bcf6998508025f9538f3377629fe9e66df6c53
Official assembly: alephium-4.6.0+5-63bcf699-SNAPSHOT.jar
Network: local devnet, network ID 2
P2P: 127.0.0.1:19973
Bootstrap peers: none
```

**Result:** the pinned official Scala application built successfully and opened the loopback P2P listener. M0 is complete. This proves the local official-peer test harness, not Rust P2P interoperability.

## 2026-08-11 — M1-001 official wire research and vector generator

**Task:** `M1-001` (`IN PROGRESS`).

**Source mapping commit:** `21318d449b7651288e14f7ab15b942e3d038362e`.

**Initial generator commit:** `07b637976bb99c3b760f2f614a9aedaae5ca0efc`.

**Added:**

- `docs/M1_WIRE_REFERENCE.md`;
- exact upstream ownership mapping for envelope, checksum, compact integers, timestamp, request ID, broker/clique identity, client ID, and networking state;
- reproducible Scala vector generator under `tools/upstream/m1/`;
- generator script, artifact workflow, deterministic test identity, malformed fixtures, and truncation table;
- copy-paste AI session prompt plus issue/PR compatibility templates.

**First generator run:** `31460717852`, conclusion `failure`.

**Exact failure:** generator referenced a non-public/nonexistent `Checksum.length`; the official companion exposes only package-private `checksumLength`.

**Fix:** commit `bfebbaccaed3edff1e1ee5760bd3647cb2b88acb` now derives the width from `Checksum.calcAndSerialize(ByteString.empty).length`, keeping the generator dependent on official behavior rather than duplicating an inaccessible constant.

**Known status:** Rust message codec and P2P remain unimplemented. M1-002 must not start until official generated vectors are reviewed and committed.

**Exact next task:**

1. make the corrected M1 vector workflow green;
2. download/review the generated artifact;
3. commit vectors plus checksums;
4. mark M1-001 `DONE` only after reproducibility evidence;
5. begin M1-002 against those vectors.

## 2026-08-11 — M1 interoperability harness and CI evidence plumbing

**Task:** create executable official-node interoperability command and evidence artifact path.

**Implementation commit:** `87d9fd1` and follow-up workflow changes.

**Changed:**

- added `make official-interop-m1` target backed by `scripts/official-node/interoperability.sh`;
- interoperability folder README under `tests/interoperability/m1/`;
- linked `docs/INTEROPERABILITY.md` and `README.md` with the new command;
- added GitHub Action `.github/workflows/m1-official-interop.yml` to run the harness in Ubuntu with Java+SBT and upload logs.

**Environment limits in this workstation:**

- `bash` shell path does not resolve Windows `git`, and `sbt` is unavailable in this local shell.
- as a result, `official-interop-m1` cannot be fully executed here yet.

**Required next commands for operator (before M1-009 can be closed):**

```bash
make official-interop-m1
```

on a Linux/WSL environment with:

- Java 17+
- sbt 1.x
- git

## 2026-08-11 — M1 interoperability smoke and flow-hardening

**Task:** continue M1 and close interoperability gate.

**Implementation commits:** `a88a778`, `7629f20`.

**Commands run in local workstation (Git Bash with local sbt):**

```bash
export PATH="/d/rustalphnode/tmp/sbt/sbt/bin:$PATH"
./scripts/official-node/build.sh
./scripts/official-node/interoperability.sh
```

**Observed outcomes:**

- Rust sends valid Hello from deterministic test identity:
  - `sent Hello: client=rust-alephium/v4.0.0/linux/p2p-v1 broker_id=0 broker_num=1`
- Rust receives official Hello:
  - `received valid Hello from scala-alephium/v4.0.0/.../p2p-v1 (1/0)`
- Rust receives official Ping and sends matching Pong across repeated cycles:
  - `received ping id=...`
  - `received matching pong N`
- Official node remained connected through the 120-second soak window and cleanly closed after run.
- `Interop run completed successfully.` printed by the harness.

**Blocker remaining for next milestone:**

- Full persistence for identity is still required for strict M1-005 scope; current Rust identity remains fixture-backed and not persisted via separate atomic storage.

## 2026-08-11 — Node identity persistence (M1-005 close)

**Task:** persist handshake identity across restarts.

**Implementation commit:** `776a430`

**Changes:**

- added `NodeIdentity` persistence helpers in `crates/conf/src/lib.rs`;
- added explicit identity path override via `RUSTNODE_IDENTITY_PATH`;
- persisted identity now loads from disk with validation and is atomically created on first run when missing;
- configured `crates/app/src/main.rs` to bootstrap `NodeConfig` with persisted identity before flow starts.

**Acceptance updates:**

- Rust handshake path still uses deterministic fixture values on first run.
- restart behavior is now backed by file persistence; identity is re-used unless storage is missing or corrupted.
