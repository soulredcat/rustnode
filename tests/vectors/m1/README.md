# M1 Scala-Derived Mainnet Vectors

Canonical fixtures are generated from the pinned official Scala source for mainnet (network id 0, magic `b0a6808e`), not from the Rust implementation. rustnode serves mainnet only (`IMMUTABLE.md`).

```bash
make m1-vectors
```

The generator:

1. checks out the pinned official Alephium commit from `UPSTREAM.toml` (v4.7.2, `76a71237e295693e760a15f3eb5d6566f2a13d93`);
2. temporarily compiles `tools/upstream/m1/RustNodeM1VectorGenerator.scala`, configured for mainnet, inside the official `tools` module;
3. writes deterministic fixtures under `tests/vectors/m1/generated`;
4. removes the temporary upstream source;
5. writes `SHA256SUMS` over the complete generated artifact.

## Committed fixtures

The committed fixtures were generated on 2026-10-27 at the pin above. Two runs gave byte-identical output, so the generator is deterministic, and the `m1-vectors` workflow regenerates them on every change to the generator, the pin, or the fixtures and requires every committed file, `manifest.tsv` included, to be byte-identical (`generated/COMMITTED_SHA256SUMS`).

The repository commits the 28 fixture files the Rust tests read, plus `COMMITTED_SHA256SUMS`. The generator also writes `truncated-hello.tsv`, a 39 KB table of every incomplete prefix of the canonical Hello frame. It is not committed because the Rust tests derive the same prefixes from `hello-frame.hex`; the generator remains its reproducible source.

## Security note

The deterministic private key emitted by the generator is public test material and must never hold funds. Its only purpose is cross-language key, broker-info, signature, and Hello fixtures.

Rust tests must not invent replacement expected values when a generated vector is inconvenient. Any fixture change requires rerunning the pinned Scala generator.
