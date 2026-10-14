# Architecture — rustnode

## 1. Mission

Serve `nixiaproject` the exact HTTP and WebSocket surface it uses (`docs/NIXIA_API_SURFACE.md`) from a node that keeps only what those endpoints need. The bot reaches every endpoint through `ALEPHIUM_NODE_URL`, so switching is a configuration change on its side.

## 2. Target shape

```text
nixiaproject ── HTTP + /ws (ALEPHIUM_NODE_URL) ──> rustnode API
                                                     ├── native handler   (once it matches official responses)
                                                     └── upstream proxy ──> official node (until then)

rustnode P2P ── Alephium peers: handshake, liveness, tx announcements and fetch, blocks
rustnode state ── pruned world state, bootstrapped from a synced official node and
                  checked against block depStateHash; recent blocks and events only
```

Migration is endpoint by endpoint. The proxy keeps the bot fully working while each native handler is compared against the official node on live traffic. The official node can be retired only when nothing is proxied any more.

## 3. Crates today

```text
crates/
├── serde/      exact Alephium wire codecs
├── crypto/     Blake2b hashing and secp256k1 handshake signatures
├── protocol/   official P2P frames and the payloads the runtime uses
├── conf/       peer, limits, retry, mempool capacity, and P2P identity configuration
├── flow/       P2P runtime
│   └── src/
│       ├── session.rs          handshake, liveness, and transaction acquisition
│       ├── request_tracker.rs  pending/in-flight/retry/dedupe lifecycle
│       ├── mempool.rs          bounded raw mempool (source for GET /mempool/transactions)
│       └── lib.rs              public composition boundary
└── app/        CLI and process lifecycle
```

`protocol` decodes only `Hello`, `Ping`/`Pong`, `InvRequest`/`InvResponse`, `ChainState`, `NewTxHashes`, `TxsRequest`, and single-item `TxsResponse`. The full official payload code table is kept so block and header messages are recognized by code and dropped. Block and header codecs come back with typed parsing and official vectors when block following is built (`TODO.md`).

Planned crates, each added only when its stage starts: `api` (HTTP and `/ws`, proxy, differential checks), `chain` (block following and fork choice), `state` (pruned world state and bootstrap), `vm` (Ralph execution).

## 4. Transaction acquisition

```text
NewTxHashes from peer
   |
   v
deduplicated + queued (bounded)
   |
   v
in-flight TxsRequest map (bounded, one transaction per request)
   |                    |
TxsResponse             timeout
   |                    |
   v                    v
raw mempool        bounded retry
(bounded, oldest   exhausted -> release
 evicted first)
```

Configuration controls the tracked-ID limit, concurrent requests, request timeout, retry count, and raw mempool capacity (default 32,000: the official `mempool-capacity-per-chain` of 2,000 times 16 chains).

The raw mempool holds unvalidated `TransactionTemplate` bytes. Serving `GET /mempool/transactions` from it also needs transaction decoding to the official JSON shape and removal of transactions once a followed block includes them.

### Latency fact

At the pinned baseline the official node announces a transaction to peers only after its own mempool accepts it, and flushes announcements in batches (`batch-broadcast-txs-frequency = 500 milli`). While this node's only peer is a local official node, it therefore sees transactions later than that node's own mempool. Acquisition only leads when connected directly to other mainnet peers.

## 5. Hot-path invariants

From `NewTxHashes` receipt until the transaction is in the raw mempool:

- no filesystem, database, or REST access;
- no JSON serialization;
- no unbounded queue or map;
- no wallet/private-key access.

## 6. Failure handling

- Peer disconnect: the session returns an error; `--watch` reconnects. The raw mempool survives reconnects.
- Request timeout: missing transaction IDs are requeued up to the retry limit.
- Unknown or malformed `TxsResponse`: ignored and logged.
- Malformed official frame or payload: the session fails closed and reconnects.
