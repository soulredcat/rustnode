# Upstream-to-Rust Mapping

## Baseline

```text
https://github.com/alephium/alephium
63bcf6998508025f9538f3377629fe9e66df6c53
```

This document maps official Scala ownership to Rust crates and records implementation status. Paths refer to the pinned commit unless explicitly updated.

Status meanings:

- `NOT STARTED`
- `PARTIAL`
- `VECTOR LOCKED`
- `IMPLEMENTED`
- `INTEROP VERIFIED`
- `PARITY VERIFIED`

## Top-level module mapping

| Official module | Rust location | Initial responsibility | Status |
|---|---|---|---|
| `util` | `crates/util` | byte/time/basic protocol utilities | `BOOTSTRAP` |
| `serde` | `crates/serde` | exact Alephium binary encoding | `BOOTSTRAP` |
| `crypto` | `crates/crypto` | official cryptographic primitives | `BOOTSTRAP` |
| `protocol` | `crates/protocol` | message/config/model/VM-visible protocol types | `BOOTSTRAP` |
| `conf` | `crates/conf` | official-compatible node/network configuration | `BOOTSTRAP` |
| `flow` | `crates/flow` | networking first; later validation, sync, BlockFlow | `BOOTSTRAP` |
| `io` | future `crates/io` | persistent storage primitives | `NOT STARTED` |
| `ralph` | future `crates/ralph` | Ralph/Alphred semantics | `NOT STARTED` |
| `api`/`ws`/`rpc`/`http` | future matching crates | external node interfaces | `NOT STARTED` |
| `wallet`/`ralphc`/`tools` | future matching crates | peripheral parity after core node | `NOT STARTED` |
| `app` | `crates/app` | executable composition/lifecycle | `BOOTSTRAP` |
| `benchmark` | future benchmark targets | post-parity measurement | `NOT STARTED` |

`BOOTSTRAP` means the crate exists but does not implement the official module yet.

## Milestone M1 source inventory

### Message envelope

| Official path | Behavior to extract | Rust target | Status |
|---|---|---|---|
| `protocol/src/main/scala/org/alephium/protocol/message/Message.scala` | outer serialization order; header/payload composition; checksum/length use; trailing-byte behavior | `crates/protocol::message`, `crates/serde` | `NOT STARTED` |
| `protocol/src/main/scala/org/alephium/protocol/message/MessageSerde.scala` | magic validation; length extraction; incremental frame boundaries; negative/incomplete length errors | `crates/serde` | `NOT STARTED` |
| `protocol/src/main/scala/org/alephium/protocol/message/Header.scala` | wire header representation and serde | `crates/protocol::message` | `NOT STARTED` |
| definition of `Checksum` under official `protocol` sources | exact checksum algorithm and serialized width/order | `crates/crypto` or `crates/protocol` after ownership review | `NOT LOCATED/LOCKED` |
| definition of `WireVersion` under official `protocol` sources | current wire version and compatibility rules | `crates/protocol::message` | `NOT LOCATED/LOCKED` |
| official network configuration files/classes | network magic bytes and selected group/network parameters | `crates/conf` | `NOT STARTED` |

The checksum, wire version, and magic bytes must be located from source/tests and locked by vectors before implementation. Do not infer them from the envelope comment.

### Payloads and handshake

| Official path | Behavior to extract | Rust target | Status |
|---|---|---|---|
| `protocol/src/main/scala/org/alephium/protocol/message/Payload.scala` | payload code table; payload-specific serializers; `Hello`, `Ping`, `Pong`; handshake validation | `crates/protocol::message` | `NOT STARTED` |
| `protocol/src/main/scala/org/alephium/protocol/message/P2PVersion.scala` | slash-separated client ID P2P version parsing | `crates/protocol::message` | `NOT STARTED` |
| `protocol/src/main/scala/org/alephium/protocol/model/ReleaseVersion.scala` | release parsing and network/hard-fork minimum checks | `crates/protocol::model` | `NOT STARTED` |
| `protocol/src/main/scala/org/alephium/protocol/model/BrokerInfo.scala` | broker/inter-broker structures, hash/serde/validation | `crates/protocol::model` | `NOT STARTED` |
| `protocol/src/main/scala/org/alephium/protocol/model/CliqueInfo.scala` | clique identity/key-related structures | `crates/protocol::model`, `crates/crypto` | `NOT STARTED` |
| official `RequestId` definition | request identifier width, generation, serde | `crates/protocol::message` | `NOT LOCATED/LOCKED` |
| official `TimeStamp` definition | unit, width, comparisons, serde | `crates/util` | `NOT LOCATED/LOCKED` |
| official signature schema/key types | exact algorithm, byte representation, sign/verify behavior | `crates/crypto` | `NOT LOCATED/LOCKED` |

### Networking state machine

| Official path | Behavior to extract | Rust target | Status |
|---|---|---|---|
| `flow/src/main/scala/org/alephium/flow/network/broker/BrokerHandler.scala` | handshake state, release/P2P validation, exchange transition, ping/pong integration, disconnect behavior | `crates/flow::network` | `NOT STARTED` |
| `flow/src/main/scala/org/alephium/flow/network/broker/ConnectionHandler.scala` | TCP buffering, frame decode loop, send/receive/error lifecycle | `crates/flow::network` | `NOT STARTED` |
| `flow/src/main/scala/org/alephium/flow/network/broker/InboundBrokerHandler.scala` | inbound handshake composition/peer behavior | `crates/flow::network` | `NOT STARTED` |
| `flow/src/main/scala/org/alephium/flow/network/broker/OutboundBrokerHandler.scala` | outbound handshake composition/peer behavior | `crates/flow::network` | `NOT STARTED` |

### Minimum upstream tests to mine for vectors

| Official path | Intended use | Status |
|---|---|---|
| `protocol/src/test/scala/org/alephium/protocol/message/PayloadSpec.scala` | payload and handshake encode/decode/validation vectors | `NOT STARTED` |
| locate official `MessageSpec`/message-serde tests | envelope, corruption and incremental decode vectors | `NOT LOCATED/LOCKED` |
| `flow/src/test/scala/org/alephium/flow/network/broker/BrokerHandlerSpec.scala` | handshake and exchange state behavior | `NOT STARTED` |
| `flow/src/test/scala/org/alephium/flow/network/broker/ConnectionHandlerSpec.scala` | TCP/frame buffering and error behavior | `NOT STARTED` |
| `flow/src/test/scala/org/alephium/flow/network/interclique/OutboundBrokerHandlerSpec.scala` | live/outbound handshake behavior | `NOT STARTED` |
| `protocol/src/test/scala/org/alephium/protocol/model/CliqueInfoSpec.scala` | clique/broker identity vectors | `NOT STARTED` |

## Mapping update rule

Whenever a task implements an upstream area:

1. replace `NOT STARTED` with the actual state;
2. add Rust file paths;
3. add vector/test paths;
4. record unresolved semantic differences;
5. do not write `PARITY VERIFIED` without differential or historical evidence.

## Future mapping order

After M1, extend this file in the same order as `TASKS.md`:

1. inventory/header/block P2P payloads;
2. protocol transaction and block models;
3. validation and hard-fork rules;
4. BlockFlow and synchronization;
5. Ralph/Alphred VM;
6. storage/recovery;
7. API/tooling.
