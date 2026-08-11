# AI/Implementation Task Template

Copy this structure into an issue, PR description, or `TASKS.md` subtask.

```markdown
## TASK-ID — Title

Status: OPEN | IN PROGRESS | BLOCKED | DONE

### Objective
One observable compatibility outcome.

### Dependencies
Earlier task IDs that must be DONE.

### Upstream baseline
- repository: https://github.com/alephium/alephium
- commit: 63bcf6998508025f9538f3377629fe9e66df6c53

### Upstream source files
- exact/path/File.scala
- exact/path/FileSpec.scala

### Observable behavior
- canonical input:
- canonical output:
- valid cases:
- invalid cases:
- network/hard-fork dependencies:
- allocation/work limits:

### Deliverables
- Rust files:
- vector files:
- test files:
- documentation updates:

### Out of scope
Explicitly list related behavior that must not be implemented in this task.

### Required commands
```bash
make check
# task-specific commands
```

### Acceptance criteria
- [ ] ...
- [ ] ...

### Evidence
- command output:
- Scala/Rust comparison:
- logs/fixtures:

### Known uncertainty
Do not hide unresolved discrepancies.

### Handoff
Exact next task and files to read.
```

A task should be small enough that another agent can review its official sources, implementation, tests, and evidence as one coherent unit.
