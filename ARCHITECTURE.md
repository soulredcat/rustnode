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

This is the shape during development. Migration is endpoint by endpoint: the proxy lets rustnode answer the whole surface while each native handler is compared against the official node. Meanwhile the bot keeps using the official node directly.

End state (N8): nothing is proxied, the proxy is deleted, the bot's `ALEPHIUM_NODE_URL` points at rustnode, and the official node is shut down. rustnode then depends only on Alephium peers; the official node's data was used once, for the bootstrap.

## 3. Crates today

```text
crates/
├── api/        HTTP and /ws gateway: allowlist, upstream proxy, per-route counters
│   └── src/
│       ├── routes.rs           the nixiaproject surface; anything else is 404/405
│       ├── proxy.rs            HTTP forward with body limit, timeout, hop-by-hop stripping
│       ├── ws.rs               /ws upgrade and frame relay in both directions
│       ├── shadow.rs           official answer to the client, native answer compared as JSON
│       ├── native/             native handlers; a handler returns None to defer to upstream
│       │   └── address.rs      asset-address group (P2PKH, P2MPKH, P2SH)
│       ├── metrics.rs          per-route counts and latency, logged every 60 s
│       └── lib.rs              listener, connection limit, header-read timeout
├── serde/      exact Alephium wire codecs
├── crypto/     Blake2b hashing and secp256k1 handshake signatures
├── model/      blocks, headers, transactions, lockup/unlock scripts, VM instructions and
│               values with the official binary encoding and hashes; checked byte for byte
│               against every block of an official node (`tests/official_raw_blocks.rs`)
├── chain/      recent blocks per chain: height-based canonical chain with the official
│               hash tie-break, tips and their pruning, flow weights, the kept window
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

Planned crates, each added only when its stage starts: `state` (pruned world state and bootstrap), `vm` (Ralph execution). Native handlers and differential checks live in `api`.

## 4. Gateway

The binary always runs the gateway; P2P acquisition runs only when a peer is given.

Bounds: request bodies up to 4 MiB (`413` above), `/ws` messages up to 16 MiB, at most 512 concurrent connections, a 10 s header-read timeout, and a 60 s upstream timeout (`504`). An unreachable official node gives `502`. Responses are streamed back, not buffered. Hop-by-hop headers are dropped in both directions; `X-API-KEY` is forwarded, including on `/ws`.

Route modes: `proxy`, `shadow`, `native` per route (`--route-mode id=mode`). A native handler starts in `shadow` and becomes the default `native` once it has matched the official node on live traffic (`native::verified`); `address-group` is native by default. In `native` mode a handler that cannot answer exactly like the official node (for example a contract address, which needs state) returns `None` and the request is proxied, so error bodies always come from the official node.

`/ws` relays text, binary, and close frames both ways. Ping and Pong stay hop-local, because each side's WebSocket library answers them itself.

Measured added latency (release build, mock upstream on loopback, 5,000 sequential `GET /mempool/transactions` per run, three runs on a 4-vCPU container): p50 56–67 µs, p95 111–118 µs, p99 126–193 µs. Reproduce with `cargo test --release -p alephium-api --test gateway -- --ignored --nocapture`.

## 5. Transaction acquisition

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

## 6. Hot-path invariants

From `NewTxHashes` receipt until the transaction is in the raw mempool:

- no filesystem, database, or REST access;
- no JSON serialization;
- no unbounded queue or map;
- no wallet/private-key access.

## 7. Failure handling

- Peer disconnect: the session returns an error; `--watch` reconnects. The raw mempool survives reconnects.
- Request timeout: missing transaction IDs are requeued up to the retry limit.
- Unknown or malformed `TxsResponse`: ignored and logged.
- Malformed official frame or payload: the session fails closed and reconnects.
