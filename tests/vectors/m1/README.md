# M1 Scala-Derived Vectors

Canonical fixtures are generated from the pinned official Scala source, not from the Rust implementation.

```bash
make m1-vectors
```

The generator:

1. checks out official Alephium commit `63bcf6998508025f9538f3377629fe9e66df6c53` (v4.6.0+5; the handshake and liveness formats it covers are unchanged in the current pin, v4.7.2);
2. temporarily compiles `tools/upstream/m1/RustNodeM1VectorGenerator.scala` inside the official `tools` module;
3. writes deterministic fixtures under `tests/vectors/m1/generated`;
4. removes the temporary upstream source;
5. writes `SHA256SUMS` over the complete generated artifact.

## Reviewed reference artifact

The reviewed generator run is recorded in `generated/ARTIFACT.tsv`:

```text
workflow run: 31461562600
artifact ID: 9090048325
workflow head: 375bb1fa3356491d69971bfd6e4a9d68a557a27a
ZIP SHA-256: 2e8f635fc2e79b3b3c4fa9cb90b924652a0b30829d6d25b29d9c829aa8b7f5b1
```

The ZIP digest was independently verified after downloading the GitHub Actions artifact, and its original `SHA256SUMS` passed for all 30 generated files.

The repository commits the 28 canonical fixture files required by Rust implementation/tests plus:

- `ARTIFACT.tsv` — workflow/artifact provenance;
- `COMMITTED_SHA256SUMS` — integrity list for the committed official fixtures.

The complete artifact also contains `truncated-hello.tsv`, a 39 KB table for every incomplete prefix of the canonical Hello frame. It is intentionally artifact-only because Rust tests can deterministically generate the same prefixes from `hello-frame.hex`; committing every repeated prefix adds no independent protocol information. The generator remains the reproducible source for the full table.

## Security note

The deterministic private key emitted by the generator is public test material and must never hold funds. Its only purpose is cross-language key, broker-info, signature, and Hello fixtures.

Rust tests must not invent replacement expected values when a generated vector is inconvenient. Any fixture change requires rerunning the pinned Scala generator and documenting the new artifact digest.
