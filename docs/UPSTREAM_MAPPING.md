# Official Alephium Mapping

Pinned source: see `UPSTREAM.toml`.

Only official areas behind code that exists in this repository are mapped here. Add a row when a stage in `TODO.md` implements a new area.

| Responsibility | Official Alephium source area | Rust owner |
| --- | --- | --- |
| Message frame, magic, checksum, payload header | `protocol/message/Message.scala` and related serde | `crates/protocol`, `crates/serde` |
| Signed `Hello` and broker identity | official `Hello`, `BrokerInfo`, crypto/signature models | `crates/protocol`, `crates/crypto`, `crates/conf` |
| `Ping` / `Pong` | official payload definitions and broker handler | `crates/protocol`, `crates/flow/src/session.rs` |
| `InvRequest` / `InvResponse` readiness exchange | official broker sync handlers | `crates/protocol`, `crates/flow/src/session.rs` |
| `NewTxHashes` | `protocol/message/Payload.scala`, inter-clique `BrokerHandler.handleNewTxHashes` | `crates/protocol`, `crates/flow/src/session.rs` |
| `TxsRequest` | official payload and broker download path | `crates/protocol`, `crates/flow/src/request_tracker.rs`, `session.rs` |
| `TxsResponse` raw transaction bytes | official payload and broker response path | `crates/protocol`, `crates/flow/src/session.rs`, `crates/flow/src/mempool.rs` |
| `ChainState` | official payload definitions | `crates/protocol`, `crates/flow/src/session.rs` |
| REST and `/ws` endpoints on the nixiaproject surface (proxied, not native) | `app/src/main/scala/org/alephium/app/RestServer.scala`, `app/src/main/scala/org/alephium/app/ws/` | `crates/api` |

## Not mapped yet

Block and header serde, PoW hashing, BlockFlow fork choice, world state, the Ralph VM, event logs, and the REST/WebSocket API are needed by later stages (`TODO.md`) and get rows when they are implemented. Wallet, mining, and explorer modules are never mapped: `nixiaproject` does not use them.
