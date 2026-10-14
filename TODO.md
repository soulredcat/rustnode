# Roadmap — nixiaproject-only node

Each stage either serves more of `docs/NIXIA_API_SURFACE.md` or removes a dependency on the official node. Nothing outside that surface is built. `nixiaproject` is never modified; it only changes `ALEPHIUM_NODE_URL` once N0 is in place.

An endpoint moves from "proxied" to "native" only after the differential check (N1) shows it matching the official node on live traffic.

## Current pointer

```text
N0 — API gateway (proxy everything to the official node)
```

## Done

- [x] Official frame codec, signed `Hello`, and `Ping`/`Pong` checked against Scala-generated vectors.
- [x] Bounded transaction acquisition (`NewTxHashes` → `TxsRequest`) with dedupe, in-flight limit, timeout, retry.
- [x] Bounded raw mempool fed by acquisition.
- [x] Removed everything the bot does not use: ALFP/IPC event server, block/header codecs without typed parsing, blake3, peer version parsing, the `util` crate, broken M2 interop modes, and stale milestone docs.

## N0 — API gateway

- [ ] `api` crate: HTTP server on the official default port, forwarding every path in the surface to a configured official node, preserving status codes, headers, bodies, and the optional API key.
- [ ] `/ws` passthrough with JSON-RPC subscriptions forwarded both ways.
- [ ] Per-endpoint latency and error counters.

Exit gate: `nixiaproject` runs unchanged against rustnode with only `ALEPHIUM_NODE_URL` changed; added latency per request is measured and documented.

## N1 — Differential checking

- [ ] Shadow mode per endpoint: serve the official response, compute the native one, and record any difference.
- [ ] A switch per endpoint from proxied to native, allowed only after a clean comparison window.

Exit gate: a native handler can be enabled and rolled back per endpoint without restarting the bot.

## N2 — Free endpoints

- [ ] `GET /addresses/{address}/group` natively (pure function of the address).
- [ ] `GET /infos/node` natively.

## N3 — Follow the chain

- [ ] Typed block and header codecs with official vectors; blake3 PoW hash.
- [ ] Header PoW and target checks; fork choice by weight across the 16 chains.
- [ ] Start from a recent checkpoint taken from the operator's synced official node.
- [ ] Keep blocks for a configured window only (default 1000 heights per chain; see the retention table in `docs/NIXIA_API_SURFACE.md`).
- [ ] Native `GET /blockflow/blocks/{hash}`, `GET /blockflow/blocks?fromTs=&toTs=` within the window, `/ws` `block` notifications.

## N4 — Native mempool

- [ ] Decode `TransactionTemplate` into the official JSON shape.
- [ ] Remove transactions once a followed block includes them; expire stale ones like the official node.
- [ ] Connect to direct mainnet peers (a local official node alone announces in 500 ms batches after its own mempool).
- [ ] Native `GET /mempool/transactions`; `GET /transactions/status` for mempool and windowed blocks.

## N5 — State bootstrap

- [ ] Read world state from a copy of the operator's synced official node data.
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
- [ ] Nothing proxied any more; measure memory and disk against the official node.
- [ ] Hard-fork readiness procedure: a new official hard fork must be implemented before its activation height.
