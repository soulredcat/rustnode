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

## N0 — API gateway

- [x] `api` crate: HTTP server (default `127.0.0.1:22973`) forwarding every path in the surface to a configured official node, preserving status codes, headers, bodies, and the optional API key; everything else `404`/`405`.
- [x] `/ws` passthrough with JSON-RPC subscriptions forwarded both ways.
- [x] Per-endpoint latency and error counters, logged every 60 s.
- [x] Integration tests against a mock official node; passthrough also checked against the real official node in `make official-interop-fastpath`.
- [x] Added latency measured: p50 56–67 µs, p99 126–193 µs (`ARCHITECTURE.md` §4).
- [x] End-to-end on a devnet with the official v4.7.2 release: every endpoint and `/ws` with real chain data, `nixiaproject`'s own client crate, restart, and load (`docs/INTEROPERABILITY.md`).
- [x] Upstream refusals keep the official status: a rejected WebSocket upgrade returns the official `401`/`404`, not `502`.
- [x] Native answers honour the official API key (`RUSTNODE_API_KEY`); other requests are forwarded.
- [ ] Optional before N8: an operator run of `nixiaproject` with only `ALEPHIUM_NODE_URL` changed, to compare native handlers on the bot's live traffic. The bot otherwise stays on the official node until N8.

Exit gate: met by the devnet end-to-end check; the measurement is done.

## N1 — Differential checking

- [x] Shadow mode per endpoint: serve the official response, compute the native one, compare as JSON, count matches/mismatches/declined, and log the first mismatches.
- [x] `--route-mode id=proxy|shadow|native` per route; a native handler defaults to `shadow` until it has matched the official node on live traffic, then to `native`; native handlers defer what they cannot answer exactly.

Exit gate: a native handler can be enabled and rolled back per endpoint without restarting the bot. Met: the switch is a rustnode flag; the bot keeps the same `ALEPHIUM_NODE_URL`.

## N2 — Free endpoints

- [x] `GET /addresses/{address}/group` natively for asset addresses (P2PKH, P2MPKH, P2SH), matching the official `AddressSpec` vectors and the official node in `make official-interop-fastpath`. Contract addresses need state and stay proxied until N5.
- [ ] `GET /infos/node` stays proxied while the official node is behind the gateway; it reports rustnode's own build info only after N8.

## N3 — Follow the chain

Done so far: block, header, and transaction encoding and JSON identical to the official node for every devnet block; P2P v2 following of an official v4.7.2 node (download by heights within its rate limit, `NewBlockHash` fetch, `ChainState` both ways, weights equal to the peer's); canonical chains, tips, weights, and the 10,000-height window; native `GET /blockflow/blocks/{hash}` for intra-group blocks.

- [ ] Mainnet P2P settings (network id, official bootstrap peers) and several public peers, so following the chain does not depend on the operator's official node.
- [ ] Typed block and header codecs with official vectors; blake3 PoW hash.
- [ ] Header PoW and target checks; fork choice by weight across the 16 chains.
- [ ] Start from a recent checkpoint taken from the operator's synced official node, never by replaying from genesis; fill the block window from that node's data, then follow new blocks from peers.
- [ ] Keep blocks for a configured window only (default 10,000 heights per chain, about 22 hours; at least 128; see the retention table in `docs/NIXIA_API_SURFACE.md`).
- [ ] Native `GET /blockflow/blocks/{hash}`, `GET /blockflow/blocks?fromTs=&toTs=` within the window, `/ws` `block` notifications.

## N4 — Native mempool

- [ ] Decode `TransactionTemplate` into the official JSON shape.
- [ ] Remove transactions once a followed block includes them; expire stale ones like the official node.
- [ ] Connect to direct mainnet peers (a local official node alone announces in 500 ms batches after its own mempool).
- [ ] Native `GET /mempool/transactions`; `GET /transactions/status` for mempool and windowed blocks.

## N5 — State bootstrap

- [ ] Read world state from a copy of the operator's synced official node data.
- [ ] Copy the permanent data the same way: DEX factory events and the sub-contract index from index 0, which the bot reads in full.
- [ ] Recompute the state root and require it to equal the next block's `depStateHash`.
- [ ] Native read-only `GET /contracts/{address}/state`, `GET /addresses/{address}/utxos`, `GET /addresses/{address}/balance` at the bootstrap point.

Exit gate: state hash matches for every group; this proves the bootstrap path before any VM work starts.

## N6 — VM and state transition

- [ ] Ralph VM for the current hard fork only (state starts after bootstrap, so historical rules are not needed).
- [ ] Apply every followed block; fail closed on any `depStateHash` mismatch.
- [ ] Prune state to the last 128 heights, like the official `PruneStorageService`, and answer pinned reads (`worldStateBlockHash`) inside that window.
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
