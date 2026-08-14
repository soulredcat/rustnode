# Architecture — Alephium Bot Fast-Path

## 1. Mission

Provide the earliest practical raw Alephium transaction signal to `alphrustalbitrage` without rebuilding consensus, world state, or the VM.

```text
Alephium P2P peer
      |
      | NewTxHashes
      v
Rust fast-path peer
      |
      | immediate bounded TxsRequest
      v
raw transaction bytes
      |
      | versioned loopback binary IPC
      v
alphrustalbitrage speculative scanner
      |
      | official-node accepted event remains mandatory
      v
build -> preflight -> policy verify -> sign -> submit
```

## 2. Trust boundary

The fast-path process is an **untrusted market-data accelerator**. It is not a consensus authority.

Allowed bot actions from fast-path events:

- decode transaction intent;
- identify affected pools and tokens;
- construct virtual reserve overlays;
- enumerate and quote affected routes;
- warm route-build inputs.

Forbidden actions from fast-path events alone:

- signing;
- broadcasting;
- changing canonical state;
- marking a transaction valid, confirmed, or final;
- bypassing official-node build/preflight checks.

## 3. Runtime components

```text
crates/
├── util/       compatibility constants and small helpers
├── serde/      exact Alephium wire codecs
├── crypto/     hashing, signatures, handshake primitives
├── protocol/   official P2P frames and payload structures
├── conf/       peer, IPC, limits, retry, and P2P identity configuration
├── flow/       bot fast-path runtime (historical crate name retained)
│   └── src/
│       ├── event.rs            versioned bot event representation
│       ├── ipc.rs              loopback TCP publisher and bounded queue
│       ├── request_tracker.rs  pending/in-flight/retry/dedupe lifecycle
│       ├── session.rs          P2P handshake, liveness, and tx acquisition
│       └── lib.rs              public composition boundary
└── app/        CLI and process lifecycle
```

The `flow` directory name is retained to avoid a low-value package/lockfile migration during the first scope cut. It no longer owns BlockFlow, validation, synchronization, or canonical state.

## 4. Hot-path invariants

From `NewTxHashes` receipt until event publication:

- no PostgreSQL access;
- no filesystem access;
- no official REST API call;
- no JSON serialization;
- no blocking on a bot consumer;
- no unbounded queue or map;
- no wallet/private-key access;
- no full transaction validation.

The IPC publisher uses a bounded `sync_channel` and `try_send`. Queue pressure drops events instead of blocking the P2P reader. Every attempted event receives a monotonically increasing process-local sequence, so the bot can detect gaps.

## 5. Transaction request lifecycle

```text
announced
   |
   v
deduplicated + queued
   |
   v
batched by chain
   |
   v
in-flight request map (bounded)
   |                    |
response                timeout
   |                    |
   v                    v
raw event          bounded retry
   |                    |
received cache       exhausted -> release
```

Configuration controls:

- maximum tracked transaction IDs;
- maximum concurrent `TxsRequest` messages;
- transaction IDs per request;
- request timeout;
- retry count;
- IPC event queue capacity.

## 6. IPC protocol

Transport: loopback TCP, one bot consumer.

Frame layout, big-endian:

```text
magic                  4 bytes  "ALFP"
version                u16      current: 1
event kind             u8
flags                  u8
sequence               u64
observed_at_unix_ns    u64
chain_from             i32
chain_to               i32
transaction_id         32 bytes
payload_length         u32
payload                N bytes
```

Event kinds:

```text
1 PeerConnected
2 TransactionAnnounced
3 RawTransaction
4 PeerDisconnected
5 Heartbeat (reserved)
```

`RawTransaction.payload` contains canonical raw bytes received from the P2P `TxsResponse` payload. A sequence gap means the bot must treat its speculative view as incomplete and rely on official-node recovery.

## 7. Failure handling

- Peer disconnect: session returns an error; `--watch` reconnects.
- Request timeout: missing transaction IDs are requeued up to the configured retry limit.
- Unknown response ID: ignored and logged.
- IPC consumer absent: events are dropped without blocking P2P.
- IPC queue full: events are dropped and counters increase.
- Malformed official frame/payload: session fails closed and reconnects.

## 8. Scope exclusions

Do not add these modules unless the repository mission is explicitly changed again:

```text
consensus
block validation
transaction validation
mempool authority
BlockFlow
Ralph/Alphred VM
world state
RocksDB canonical storage
wallet
miner
REST API parity
explorer/indexer
```
