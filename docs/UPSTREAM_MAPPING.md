# Official Alephium Mapping — Bot Fast-Path Scope

Pinned source: see `UPSTREAM.toml`.

Only symbols required by the transaction fast path are mapped here.

| Fast-path responsibility | Official Alephium source area | Rust owner |
| --- | --- | --- |
| Message frame, magic, checksum, payload header | `protocol/message/Message.scala` and related serde | `crates/protocol`, `crates/serde` |
| Signed `Hello` and broker identity | official `Hello`, `BrokerInfo`, crypto/signature models | `crates/protocol`, `crates/crypto`, `crates/conf` |
| `Ping` / `Pong` | official payload definitions and broker handler | `crates/protocol`, `crates/flow/src/session.rs` |
| `InvRequest` / `InvResponse` readiness exchange | official broker sync handlers | `crates/protocol`, `crates/flow/src/session.rs` |
| `NewTxHashes` | `protocol/message/Payload.scala`, inter-clique `BrokerHandler.handleNewTxHashes` | `crates/protocol`, `crates/flow/src/session.rs` |
| `TxsRequest` | official payload and broker download path | `crates/protocol`, `crates/flow/src/request_tracker.rs`, `session.rs` |
| `TxsResponse` raw transaction bytes | official payload and broker response path | `crates/protocol`, `crates/flow/src/session.rs` |
| Transaction ID hashing used for correlation | official transaction ID semantics | `crates/crypto`, `request_tracker.rs` |

## Explicitly unmapped

The following official modules are outside this repository's mission:

```text
BlockFlow
block/header validation
mempool authority
PoW/PoLW validation
Ralph/Alphred VM
world state
canonical persistence
wallet
mining
REST/API parity
indexer/explorer
```

Do not add mapping placeholders for those modules. Their absence is intentional.
