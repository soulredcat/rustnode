# Roadmap — nixiaproject-only node

Each stage either serves more of `docs/NIXIA_API_SURFACE.md` or removes a dependency on the official node. Nothing outside that surface is built. `nixiaproject` is never modified.

End state: rustnode is the bot's only node and the official node is shut down. Until then the bot keeps using the official node directly; rustnode is built alongside it and checked against it. The official node's data is used once, for the bootstrap (N3, N5); after that rustnode follows the chain from Alephium peers on its own.

An endpoint moves from "proxied" to "native" only after the differential check (N1) shows it matching the official node.

## Current pointer

```text
N3 — Follow the chain
```

## Done

- [x] Official frame codec, signed `Hello`, and `Ping`/`Pong` checked against Scala-generated vectors.
- [x] Bounded transaction acquisition (`NewTxHashes` → `TxsRequest`) with dedupe, in-flight limit, timeout, retry.
- [x] Bounded raw mempool fed by acquisition.
- [x] Removed everything the bot does not use: ALFP/IPC event server, block/header codecs without typed parsing, blake3, peer version parsing, the `util` crate, broken M2 interop modes, and stale milestone docs.
- [x] Mainnet only (`IMMUTABLE.md`): every non-mainnet path, the old interop harness and its CI job, and `--network-id` deleted; M1 vectors regenerated from the pinned official code for mainnet; dead code swept; `make mainnet-check` replaces the devnet run.

## N0 — API gateway

- [x] `api` crate: HTTP server (default `127.0.0.1:22973`) forwarding every path in the surface to a configured official node, preserving status codes, headers, bodies, and the optional API key; everything else `404`/`405`.
- [x] `/ws` passthrough with JSON-RPC subscriptions forwarded both ways.
- [x] Per-endpoint latency and error counters, logged every 60 s.
- [x] Integration tests against a mock official node; passthrough also checked against the official mainnet node in `make mainnet-check`.
- [x] Added latency measured: p50 56–67 µs, p99 126–193 µs (`ARCHITECTURE.md` §4).
- [x] Gateway checked against the official mainnet node on every `make mainnet-check`: official build info, refusal outside the surface, and native address groups (`docs/MAINNET_CHECK.md`).
- [x] Upstream refusals keep the official status: a rejected WebSocket upgrade returns the official `401`/`404`, not `502`.
- [x] Native answers honour the official API key (`RUSTNODE_API_KEY`); other requests are forwarded.
- [ ] Optional before N8: an operator run of `nixiaproject` with only `ALEPHIUM_NODE_URL` changed, to compare native handlers on the bot's live traffic. The bot otherwise stays on the official node until N8.

Exit gate: met by `make mainnet-check` against the official mainnet node; the measurement is done.

## N1 — Differential checking

- [x] Shadow mode per endpoint: serve the official response, compute the native one, compare as JSON, count matches/mismatches/declined, and log the first mismatches.
- [x] `--route-mode id=proxy|shadow|native` per route; a native handler defaults to `shadow` until it has matched the official node on live traffic, then to `native`; native handlers defer what they cannot answer exactly.

Exit gate: a native handler can be enabled and rolled back per endpoint without restarting the bot. Met: the switch is a rustnode flag; the bot keeps the same `ALEPHIUM_NODE_URL`.

## N2 — Free endpoints

- [x] `GET /addresses/{address}/group` natively for asset addresses (P2PKH, P2MPKH, P2SH), matching the official `AddressSpec` vectors and the official mainnet node in `make mainnet-check`. Contract addresses need state and stay proxied until N5.
- [ ] `GET /infos/node` stays proxied while the official node is behind the gateway; it reports rustnode's own build info only after N8.

## N3 — Follow the chain

Done so far: block, header, and transaction encoding and JSON identical to the official mainnet node for its newest blocks (`make mainnet-check`); P2P v2 following of the official v4.7.2 mainnet node (download by heights within its rate limit, `NewBlockHash` fetch, `ChainState` both ways, weights equal to the peer's); canonical chains, tips, weights, and the 100,000-height window on disk; native `GET /blockflow/blocks/{hash}` for intra-group blocks.

- [x] Mainnet P2P settings (network id, official bootstrap peers) and several public peers, so following the chain does not depend on the operator's official node: `--follow` (and a repeatable `--peer`) follows from the six official `bootstrap{0..5}.alephium.org:9973` nodes over inter-clique TCP, no UDP discovery yet; each peer runs its own follow worker sharing the chain store and mempool (`conf::BOOTSTRAP_PEERS`, `app`).
- [x] Fetch an announced block only when its hash carries mainnet's minimum work (37 leading zero bits, `network::MIN_HASH_ZERO_BITS`), so a peer cannot trigger fetches with hashes it did not mine.
- [x] Typed block and header codecs, byte for byte against the official mainnet node; blake3 PoW hash.
- [x] Header validation of every block from a peer, as the official `HeaderValidation` (`ChainStore::validate`): version, increasing timestamp, 15 s drift, dependency count and order, PoW, the Danube difficulty adjustment with the penalty patch (`chain::difficulty`), uncle dependency timestamps, and the flow check. Checked on 540 mainnet blocks of all 16 chains (`official_headers`). The state hash check comes with state (N6).
- [ ] Fork choice by weight across the 16 chains (best dependencies, the state view without `worldStateBlockHash`): needed with state (N5).
- [x] Start from a recent checkpoint taken from the operator's synced official node, never by replaying from genesis: the window is read from its REST API (`raw-blocks`), weights are seeded from a peer's `ChainState` and derived backwards along intra-group chains and forwards with `calWeight`; then new blocks are followed over P2P.
- [x] Keep blocks for a configured window only (`--window`, default 100,000 heights per chain, about 9 days; at least 128; see the retention table in `docs/NIXIA_API_SURFACE.md`).
- [x] Keep the window on disk (`crates/store`, one redb file, `--data`) with only the newest 1,024 heights per chain in memory. A restart reopens it and fetches only the missed heights: from the official node when it agrees with the stored chain (same genesis, same canonical block 200 heights below the lower tip, at most a window behind), otherwise from peers. Commits are fsynced at least every 10 s; a crash loses at most that, and peers bring it back.
- [ ] Native `GET /blockflow/blocks/{hash}` for inter-group blocks, `GET /blockflow/blocks?fromTs=&toTs=` within the window, and `/ws` `block` notifications: since Danube an inter-group block's JSON carries `conflictedTxs`, which the official node computes when it applies the block to state, so these wait for N6. Blocks by time also need a timestamp-to-height lookup on disk.

## N4 — Native mempool

- [x] Decode `TransactionTemplate` into the official JSON shape (with `seenAt`).
- [x] Remove transactions once a block holding them enters the best view of their group, return them when it leaves (Danube best view: `calBestFlowSkeletonUnsafe`, `calMemPoolChangesUnsafe`); skip relayed transactions already confirmed; expire after 10 minutes like the official node.
- [ ] Connect to direct mainnet peers (a local official node alone announces in 500 ms batches after its own mempool).
- [x] Transaction index of the window on disk (`txs` table: id to the blocks holding it), kept by add, checkpoint import and pruning.
- [x] Native `GET /transactions/status` for transactions confirmed in intra-group blocks of the window, searched in the official chain order with and without groups (`shadow`; 36 of 36 equal in `make mainnet-check`; confirmations may trail the official node by up to 2 blocks, as between any two nodes). `MemPooled` and `TxNotFound` wait for the native mempool; inter-group blocks can hold `Conflicted` transactions and wait for state (N6).
- [x] Native `GET /mempool/transactions`; `MemPooled` and `TxNotFound` statuses (`shadow`; 84 of 84 mempools with 116 transactions and 54 of 54 statuses equal in `make mainnet-check`). Statuses are declined until every searched chain has a weight from the peer's `ChainState`. A pruned node does not find transactions confirmed before its window.

## N5 — State bootstrap

- [ ] Read world state from a copy of the operator's synced official node data.
- [ ] Copy the permanent data the same way: DEX factory events and the sub-contract index from index 0, which the bot reads in full.
- [ ] Recompute the state root and require it to equal the next block's `depStateHash`.
- [ ] Native read-only `GET /contracts/{address}/state`, `GET /addresses/{address}/utxos`, `GET /addresses/{address}/balance` at the bootstrap point.

Exit gate: state hash matches for every group; this proves the bootstrap path before any VM work starts.

## N6 — VM and state transition

- [ ] Ralph VM for the current hard fork only (state starts after bootstrap, so historical rules are not needed).
- [ ] Apply every followed block; fail closed on any `depStateHash` mismatch.
- [ ] Keep the state of every height in the block window (100,000 per chain), on disk next to it, and answer pinned reads (`worldStateBlockHash`) anywhere inside it: the bot's CLAMM audit tools replay quotes at blocks recorded days earlier. Trie nodes are shared between heights; a node goes only when no height left in the window reaches it.
- [ ] Keep spent outputs for retained blocks (for `rich-details`), DEX factory events permanently, and the sub-contract index permanently.
- [ ] Events for the indexes on the surface only; native `/events/*`, `blocks-with-events`, `sub-contracts`, `/ws` `contract` notifications.

## N7 — VM calls and transaction building

- [ ] Native `call-contract`, `call-tx-script`, `multicall-contract`.
- [ ] Native `decode-unsigned-tx`, `contracts/unsigned-tx/execute-script`, `transactions/build`, with the official gas estimation and UTXO selection.
- [ ] Native `transactions/rich-details`.

## N8 — Submission and retirement of the official node

- [ ] Native `transactions/submit`: validate and broadcast to peers.
- [ ] Nothing proxied any more: delete the upstream proxy and shadow mode; measure memory and disk against the official node.
- [ ] Switch: `nixiaproject`'s `ALEPHIUM_NODE_URL` points at rustnode, and the operator shuts the official node down.
- [ ] Hard-fork readiness procedure: a new official hard fork must be implemented before its activation height.
