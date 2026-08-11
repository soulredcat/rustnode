# Alephium Rust Node

> Rust rebuild of the official Alephium node.

The goal of this repository is straightforward: **make a working Alephium node in Rust that can communicate and operate with the existing official Alephium Scala node.**

The initial project is not a new Alephium protocol and not an optimization project. We first reproduce the official behavior in Rust. Optimization comes only after the Rust implementation works correctly with the official network.

## Source of truth

Official implementation:

- Repository: https://github.com/alephium/alephium
- Baseline commit: `63bcf6998508025f9538f3377629fe9e66df6c53`
- Baseline date: 2026-07-22

The Scala implementation is the reference behavior during the rebuild.

## First objective

The first useful milestone is **not** a complete Rust full node.

It is this:

```text
Official Alephium Node (Scala)
          ^
          | Alephium P2P protocol
          v
     Rust Node
```

The Rust binary must be able to:

1. start with Alephium network configuration;
2. open a TCP P2P connection to an official node;
3. encode/decode the official Alephium message envelope;
4. exchange a valid signed `Hello` handshake;
5. be accepted as an Alephium peer;
6. exchange `Ping` / `Pong`;
7. remain connected without a protocol adapter or proxy.

After that works, functionality is added vertically until the Rust node can fully synchronize and validate the chain.

## Current status

**Bootstrap / interoperability implementation.**

There is not yet a production-ready Rust node in this repository.

## Rules for the initial rebuild

Until compatibility is complete:

```text
Official Alephium behavior
          |
          v
     Rust equivalent
```

Do not introduce protocol changes while rebuilding.

That means no changes to:

- BlockFlow consensus;
- PoW / PoLW rules;
- P2P semantics;
- transaction format;
- block format;
- hashing;
- signatures;
- Ralph / Alphred VM semantics;
- gas accounting;
- hard-fork rules;
- group/shard rules;
- block limits;
- transaction limits;
- tokenomics or fee distribution.

Performance optimization is explicitly deferred until the Rust node reaches official parity.

## Official module mapping

The official repository is already modular. The Rust rebuild should preserve those responsibilities as closely as practical rather than inventing a new protocol architecture during the port.

Important upstream modules include:

```text
macros
util
serde
io
crypto
protocol
flow
json
conf
api
ws
rpc
http
ralph
wallet
ralphc
app
tools
benchmark
integration
```

Planned Rust workspace:

```text
rustnode/
├── Cargo.toml
├── crates/
│   ├── macros/
│   ├── util/
│   ├── serde/
│   ├── io/
│   ├── crypto/
│   ├── protocol/
│   ├── flow/
│   ├── json/
│   ├── conf/
│   ├── api/
│   ├── ws/
│   ├── rpc/
│   ├── http/
│   ├── ralph/
│   ├── wallet/
│   ├── ralphc/
│   ├── app/
│   ├── tools/
│   └── benchmark/
├── integration/
├── tests/
│   ├── vectors/
│   ├── interoperability/
│   ├── differential/
│   └── replay/
└── ARCHITECTURE.md
```

Not every crate needs to be implemented immediately. Development follows a **working vertical slice**: implement the minimum official components required to connect to a Scala node, then extend the same implementation toward synchronization, validation, VM execution, storage, API compatibility, and finally full-node parity.

## Interoperability milestone 1

The official P2P connection starts with a signed `Hello` message containing:

- `clientId`;
- timestamp;
- broker information;
- signature.

The official node validates the handshake, derives the peer's release and P2P version from `clientId`, and then switches the connection into the appropriate P2P message exchange mode.

The Rust implementation therefore needs, before anything else:

```text
util
  |
serde + crypto
  |
protocol/message
  |
flow/network minimum
  |
app binary
```

The first executable target is:

```text
cargo run ...
     |
     v
connect official node
     |
     v
Hello <-> Hello
     |
     v
Ping <-> Pong
     |
     v
stable accepted peer
```

## Implementation order

### M0 - Rust workspace boots

- Cargo workspace;
- configuration loading required for the selected network;
- logging and controlled shutdown;
- node identity/key material required by the official handshake.

### M1 - Official P2P handshake works

- official message framing;
- magic bytes;
- checksum;
- payload length;
- payload code;
- `Hello` serialization/deserialization;
- handshake signature validation;
- official-compatible `clientId` parsing/format;
- P2P v1/v2 selection as required by upstream;
- connect Rust <-> Scala;
- `Ping` / `Pong`.

**Exit condition:** the official Scala node accepts the Rust node as a peer and the connection stays healthy.

### M2 - Basic protocol exchange

Implement the official payloads needed for synchronization, including the corresponding request IDs and serializers:

- chain state/inventory messages;
- header requests/responses;
- block requests/responses;
- new header/block announcements;
- transaction inventory/request/response messages.

**Exit condition:** Rust can request and decode real headers/blocks from an official node.

### M3 - Protocol models and cryptography

- addresses and IDs;
- transactions;
- outputs;
- block headers;
- blocks;
- Merkle/hash behavior;
- all protocol-required cryptography.

**Exit condition:** Rust derives the same hashes/IDs as Scala for captured network data.

### M4 - Validation and BlockFlow

- official header validation;
- official block validation;
- official transaction validation;
- chain/group handling;
- BlockFlow dependencies, weights, views, and chain selection;
- hard-fork-specific behavior.

**Exit condition:** Rust accepts/rejects the same real chain data as Scala.

### M5 - VM and state

- Ralph/Alphred VM behavior;
- stack/frame/instruction semantics;
- gas accounting;
- contract state;
- world-state transitions;
- official storage behavior at the logical level.

**Exit condition:** Rust produces identical VM/state results for official test vectors and historical blocks.

### M6 - Full synchronization

- persistent storage;
- restart/recovery;
- mempool;
- complete block/header synchronization;
- genesis-to-tip replay.

**Exit condition:** Rust syncs from genesis and reaches the same valid chain state as an official node.

### M7 - API/tooling parity

- required HTTP API;
- WebSocket support;
- RPC behavior;
- wallet/compiler/tools where required by project scope.

### M8 - Optimization

Only after M1-M7 parity is proven:

- profile first;
- optimize Rust implementation without changing protocol behavior;
- benchmark against the official Scala node on equivalent hardware;
- only then research protocol-level improvements separately.

## Compatibility testing

Every consensus- or wire-critical component should be tested against the official implementation.

```text
same bytes / same state
       |
   +---+---+
   |       |
 Scala    Rust
   |       |
   +---==--+
```

Required test classes:

- official/golden serialization vectors;
- Rust <-> Scala P2P interoperability tests;
- differential transaction/block validation;
- differential VM execution;
- historical block replay;
- restart/recovery tests.

The Rust node is considered correct only when its consensus-visible results match the official implementation.

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md).

## License

No project license has been selected yet.

The official Alephium repository is licensed under LGPL-3.0. Licensing/compliance must be resolved before mechanically copying or porting upstream source code.

## Disclaimer

This project is experimental and unofficial until compatibility with the existing Alephium network has been demonstrated and independently validated.
