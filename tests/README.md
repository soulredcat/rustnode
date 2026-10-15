# Test Layout

Tests are evidence of compatibility, not only Rust implementation quality.

Current layout:

```text
tests/
├── vectors/m1/                Scala-generated wire vectors (handshake, liveness, framing)
└── interoperability/fastpath/ live run against the pinned official node
```

Later stages in `TODO.md` add vectors for block/header and transaction serde, and a `differential/` record of native API responses compared against the official node.

## Vector provenance

Every externally visible expected result should record:

- pinned upstream commit;
- upstream generator/test/source path;
- network/hard-fork context;
- generation command;
- whether the value is official-generated, network-captured, or manually constructed.

Do not treat a Rust encode/decode round trip as proof of Scala parity.

## Sensitive data

Never commit:

- private keys;
- wallet seeds;
- API secrets;
- unredacted production logs containing secrets;
- private peer infrastructure details not intended for publication.

Use deterministic test-only keys/fixtures clearly marked as public test material.
