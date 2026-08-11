# Milestone M1 — Official P2P Interoperability

## Objective

Produce a Rust binary that establishes and maintains a valid Alephium P2P session with a locally controlled official Scala node at the pinned baseline.

M1 proves only networking/handshake interoperability. It does not prove block validation, synchronization, VM parity, or full-node correctness.

## Required topology

```text
+---------------------------+           +---------------------------+
| Pinned official node      |    TCP    | Rust bootstrap node       |
| Scala                     |<--------->| Rust                      |
| local controlled process  |   P2P     | no translation proxy      |
+---------------------------+           +---------------------------+
```

Malformed-frame tests must target a controlled local official process, not unrelated public mainnet peers.

## Official behavior sources

Minimum pinned files:

```text
protocol/.../message/Message.scala
protocol/.../message/MessageSerde.scala
protocol/.../message/Header.scala
protocol/.../message/Payload.scala
protocol/.../message/P2PVersion.scala
protocol/.../model/ReleaseVersion.scala
protocol/.../model/BrokerInfo.scala
protocol/.../model/CliqueInfo.scala
flow/.../network/broker/ConnectionHandler.scala
flow/.../network/broker/BrokerHandler.scala
flow/.../network/broker/InboundBrokerHandler.scala
flow/.../network/broker/OutboundBrokerHandler.scala
```

Use the exact complete paths in `docs/UPSTREAM_MAPPING.md` and extend that mapping as additional definitions are located.

## Wire envelope

At the pinned baseline, `Message.serialize` conceptually composes:

```text
network magic
|| checksum(message data)
|| encoded message-data length
|| message data

message data = wire header || encoded payload
encoded payload = payload code || payload body
```

This is a reading aid, not a substitute for vectors. M1-001 must lock:

- exact byte order;
- exact widths;
- exact integer encoding;
- exact checksum algorithm;
- exact network magic for the selected test network;
- maximum accepted frame/payload size;
- incomplete-frame behavior;
- trailing-byte behavior.

Do not implement from the comment in `Message.scala` alone. Follow executable serialization and tests.

## Initial payloads

M1 supports only the payloads required to establish liveness:

```text
Hello
Ping
Pong
```

The payload code table must match official ordering exactly.

### Hello fields

```text
client_id
timestamp
broker_info
signature
```

Official validation includes broker information validation, a signature over the official broker-info hash bytes, and timestamp checks. The Rust implementation must use the same algorithm, key representation, hashing, and encoded structure.

### Client ID

The official node derives release compatibility and P2P version from slash-separated `clientId` fields.

The Rust client must identify itself honestly while remaining parser-compatible. The exact value is not chosen until M1-001 establishes:

- the compatible release field for the selected network/hard-fork state;
- whether P2P v1, v2, or both are implemented;
- accepted segment layout.

Do not advertise `p2p-v2` until its required behavior is implemented.

## Connection state machine

Required conceptual state:

```text
Disconnected
  -> Connecting
  -> TcpConnected
  -> HelloSent
  -> Established
  -> Closing
  -> Disconnected
```

Events include:

- connect success/failure;
- local `Hello` encoded/sent;
- valid/invalid remote `Hello`;
- handshake timeout;
- frame decode error;
- EOF/socket error;
- Ping timer;
- Ping/Pong correlation;
- controlled shutdown.

No synchronization payload may be treated as trusted/canonical state in M1.

## Incremental TCP decoding

TCP does not preserve message boundaries. Tests must cover:

1. one frame in one read;
2. one frame split at every envelope boundary;
3. one frame split inside every variable field represented in vectors;
4. several frames in one read;
5. a complete frame followed by a partial frame;
6. malformed frame followed by bytes that resemble a valid frame;
7. EOF with a partial frame;
8. excessive declared length before allocation.

The decoder must return a clear state such as:

```text
NeedMoreData
Decoded(frame, consumed_bytes)
ProtocolError(error)
```

The exact Rust API can differ, but behavior must remain bounded and deterministic.

## Vector requirements

Create Scala-derived fixtures under:

```text
tests/vectors/m1/
```

Recommended manifest fields:

```text
name
upstream_commit
upstream_generator_path
network
input_hex
expected_result
expected_consumed_bytes
notes
```

Required valid fixtures:

- official header;
- `Hello` payload;
- full `Hello` frame;
- `Ping` payload/frame;
- `Pong` payload/frame;
- Scala signature verified by Rust;
- Rust signature verified by Scala.

Required invalid fixtures:

- wrong magic;
- wrong checksum;
- negative/invalid length where representable by official encoding;
- declared length greater than available data;
- invalid payload code;
- trailing payload bytes;
- invalid UTF-8/client ID length cases according to upstream;
- invalid broker info;
- invalid signature;
- invalid timestamp;
- incompatible release/client ID;
- unsupported P2P version.

## Identity persistence

Handshake identity must not regenerate on every restart once persistence is implemented.

Minimum properties:

- atomic create-or-load;
- deterministic validation of stored bytes;
- restrictive file permissions where the platform supports them;
- no private key in logs/errors;
- explicit test-only identity path;
- corruption causes a clear startup failure rather than silent replacement.

The exact storage format must be documented before use.

## Local official-node runbook

M0-004 must create `docs/LOCAL_OFFICIAL_NODE.md` with verified commands for the pinned commit.

The runbook must state:

- Java/SBT requirements used;
- official build command;
- isolated `ALEPHIUM_HOME`/configuration path;
- selected network and group settings;
- P2P listen address/port;
- log level and expected handshake log lines;
- startup readiness check;
- shutdown/cleanup;
- exact command used by the Rust client.

Do not put private keys or credentials in the repository.

## Interoperability acceptance gate

M1 passes only when all are true:

1. official Scala node starts from a documented clean setup;
2. Rust opens TCP directly to its P2P listener;
3. both sides exchange valid official frames;
4. Scala accepts Rust `Hello` and enters exchange mode;
5. Rust validates Scala `Hello` and enters `Established`;
6. required Ping/Pong exchange succeeds;
7. connection stays healthy for the documented soak interval;
8. clean shutdown works;
9. invalid local test cases fail safely;
10. logs and commands are attached to the task evidence with secrets removed.

Suggested initial soak interval: 10 minutes. This is an M1 liveness check, not a production reliability guarantee.

## Required evidence files

Recommended output:

```text
tests/vectors/m1/...
tests/interoperability/m1/README.md
tests/interoperability/m1/official-node.log.redacted
tests/interoperability/m1/rust-node.log.redacted
tests/interoperability/m1/commands.txt
tests/interoperability/m1/environment.txt
```

Large or unstable generated artifacts should be stored according to a documented artifact policy rather than committed blindly.

## Out of scope

M1 does not implement or claim:

- peer discovery parity;
- header/block synchronization;
- transaction relay completeness;
- transaction/block validation;
- canonical chain selection;
- world-state execution;
- mining;
- wallet/API compatibility;
- performance advantage.
