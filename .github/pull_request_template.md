## Task

`TASK-ID`

## Upstream baseline

- Repository: `https://github.com/alephium/alephium`
- Commit: `63bcf6998508025f9538f3377629fe9e66df6c53`

## Upstream sources and tests consulted

- `exact/path/File.scala`
- `exact/path/FileSpec.scala`

## Observable behavior reproduced

Describe inputs, outputs, accepted/rejected cases, and any network/hard-fork context.

## Changes

- 

## Evidence

- [ ] Scala-derived vector or captured official fixture
- [ ] Rust unit/property tests
- [ ] Rust/Scala interoperability or differential evidence when required
- [ ] No expected protocol bytes invented only from Rust round trips

Evidence paths/run IDs:

- 

## Commands executed

```bash
make check
# task-specific commands
```

Results:

```text
paste concise real output/status
```

## Safety and compatibility checklist

- [ ] The change does not introduce a protocol redesign or optimization outside the task.
- [ ] Peer-controlled sizes/work are bounded.
- [ ] Production paths do not panic on untrusted input.
- [ ] No floating-point arithmetic enters protocol/consensus calculations.
- [ ] No generic serializer is used for official Alephium wire bytes.
- [ ] Rust status language remains honest (`decoded`, `verified`, `validated`, `committed`, `interoperable`).
- [ ] `TASKS.md` is updated when status changed.
- [ ] `docs/UPSTREAM_MAPPING.md` is updated when an upstream area was implemented.
- [ ] `docs/WORKLOG.md` contains a handoff entry.

## Known limitations or uncertainty

- 

## Exact next task

`TASK-ID` — description
