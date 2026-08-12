# AI Session Prompt

Use this prompt when assigning repository work to Codex or another capable coding AI. Replace only the optional final scope line when necessary.

```text
You are working on the private repository soulredcat/rustnode.

Mission:
Build a Rust implementation of the official Alephium node that is protocol- and consensus-compatible with the existing Scala implementation. Compatibility comes before optimization. The product goal is a working Rust full node that communicates directly with the pinned official node, synchronizes from genesis, validates the same chain, executes the same VM/state transitions, persists/restarts safely, and derives the same canonical state.

Official compatibility baseline:
https://github.com/alephium/alephium
commit 63bcf6998508025f9538f3377629fe9e66df6c53

Before changing anything, read these files in order:
1. README.md
2. AGENTS.md completely
3. TODO.md
4. TASKS.md
5. ARCHITECTURE.md
6. docs/UPSTREAM_MAPPING.md
7. the milestone/runbook document referenced by the active TODO subphase
8. docs/AI_HANDOFF.md
9. the latest relevant docs/WORKLOG.md entry

Then inspect Git status, current HEAD, recent commits, repository tree, existing tests, and current CI/workflow state.

Execution authority:
- The pinned official Scala implementation is the behavioral source of truth.
- TODO.md is authoritative for milestone/phase/subphase ordering and the current execution pointer.
- TASKS.md is authoritative for detailed acceptance criteria and evidence.
- Work on exactly the subphase named by TODO.md under Current execution pointer.
- Do not select another open task because it looks easier.
- Do not start a later phase or milestone while an earlier required subphase remains unchecked.
- Exactly one TODO subphase should be ACTIVE.

Non-negotiable restrictions during compatibility work:
- no consensus changes;
- no transaction/block/wire-format changes;
- no fee redistribution or service-node implementation;
- no new staking or voting role;
- no changed Ralph/Alphred VM semantics;
- no changed gas, hard-fork, group, shard, block, or transaction limits;
- no guessed constants, field order, byte order, checksum, hash preimage, signature format, timeout, or network configuration;
- no optimization without parity evidence and profiling;
- no unbounded allocation, retry, queue, buffer, recursion, or concurrency reachable from untrusted input;
- no panics on untrusted network or disk input;
- no claim of success without executing the required tests;
- no claim that decoded data is validated or that a handshake client is a full node.

Required workflow:
1. State the active milestone, phase, subphase ID, title, dependencies, and exact acceptance gate.
2. Verify prerequisites are complete in TODO.md.
3. Inspect and record exact pinned upstream files, symbols, and tests that define the behavior.
4. Define observable compatibility requirements before implementation.
5. Add or use Scala-derived vectors, differential tests, historical replay, crash/recovery evidence, or direct official-node interoperability as required.
6. Implement the smallest complete behavior for the active subphase only.
7. Keep deterministic codec/validation/VM/state logic synchronous; use async only at I/O, timer, cancellation, and orchestration boundaries.
8. Use typed errors and fail closed.
9. Keep all input lengths, buffers, channels, retries, retained state, and concurrency bounded.
10. Run the subphase-specific commands plus:

cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace

11. Do not mark the subphase complete if an external fixture, official-node test, differential test, replay, or recovery gate was not actually executed.
12. On completion, check the subphase in TODO.md, remove ACTIVE from it, and move the Current execution pointer/ACTIVE marker to the first incomplete unblocked subphase.
13. Update TASKS.md acceptance/evidence, docs/UPSTREAM_MAPPING.md when relevant, append docs/WORKLOG.md, and update docs/AI_HANDOFF.md.
14. Commit coherent, reviewable changes with the subphase ID in the commit message.

At the start of your response, state only:

Active subphase: <ID and title>
Dependencies: <checked prerequisite IDs>
Upstream files to inspect: <paths/symbols>
Acceptance gate: <exact commands and expected evidence>

Then inspect and implement. Do not ask for confirmation when repository documents already define the answer.

If blocked:
- prove the blocker with exact output/evidence;
- complete every independent part still permitted by the active subphase;
- leave its TODO checkbox open;
- record the blocker in TASKS.md, docs/WORKLOG.md, and docs/AI_HANDOFF.md;
- do not jump to another milestone.

At the end, report:

Subphase status:
Files changed:
Upstream symbols inspected:
Behavior implemented:
Commands executed and exact result:
Parity/interoperability/replay/recovery evidence:
Known gaps:
TODO pointer after this session:
Next exact subphase:
Commit SHA:

Optional requested scope for this session:
<leave empty to follow TODO.md, or insert the exact currently active subphase ID>
```

## Valid scope example

```text
M2-P0-S1 only. Apply rustfmt, verify the diff is formatting-only, and pass cargo fmt --all -- --check. Do not fix compile or interoperability issues until the TODO pointer advances.
```

## Invalid scope examples

```text
Make Alephium faster.
```

```text
Rewrite the entire official repository in one pass.
```

```text
Skip M2 and start the VM.
```

```text
Implement a Rust node based on general blockchain knowledge.
```

The ordered TODO exists specifically to prevent ambiguous assignments, duplicate work, and milestone jumping.
