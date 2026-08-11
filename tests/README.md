# Test Layout

Tests are evidence of compatibility, not only Rust implementation quality.

Planned layout:

```text
tests/
├── vectors/
│   ├── m1/
│   ├── protocol/
│   ├── validation/
│   └── vm/
├── interoperability/
│   ├── m1/
│   └── sync/
├── differential/
├── replay/
└── recovery/
```

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
