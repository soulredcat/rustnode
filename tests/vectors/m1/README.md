# M1 Scala-Derived Vectors

Canonical fixtures are generated from the pinned official Scala source, not from the Rust implementation.

```bash
make m1-vectors
```

The generator:

1. checks out official Alephium commit `63bcf6998508025f9538f3377629fe9e66df6c53`;
2. temporarily compiles `tools/upstream/m1/RustNodeM1VectorGenerator.scala` inside the official `tools` module;
3. writes deterministic fixtures under `tests/vectors/m1/generated`;
4. removes the temporary upstream source;
5. writes `SHA256SUMS` over all generated files.

The deterministic private key emitted by the generator is public test material and must never hold funds.

`generated/` is committed only after the generator workflow succeeds and the outputs are reviewed. Rust tests must not invent replacement expected values when a generated vector is inconvenient.
