# rustnode

An Alephium node in Rust, built only for the `soulredcat/nixiaproject` arbitrage bot.

The goal is a node that serves exactly the HTTP and WebSocket endpoints `nixiaproject` calls ([`docs/NIXIA_API_SURFACE.md`](docs/NIXIA_API_SURFACE.md)), with the same responses as the official node, while keeping only the state, blocks, and indexes those endpoints need. The bot switches by changing `ALEPHIUM_NODE_URL` and nothing else; `nixiaproject` itself is never modified from here.

Anything the bot does not use is not built, and unused code is deleted.

## Current status

Available today:

- official Alephium frame codec and the payloads the runtime uses (`Hello`, `Ping`/`Pong`, `InvRequest`/`InvResponse`, `ChainState`, `NewTxHashes`, `TxsRequest`/`TxsResponse`), checked against Scala-generated vectors;
- signed `Hello` handshake and `Ping`/`Pong` liveness against the official node;
- bounded transaction acquisition (`NewTxHashes` → `TxsRequest`) with dedupe, in-flight limits, timeouts, and retries;
- a bounded in-memory raw mempool that `GET /mempool/transactions` will be served from.

Not built yet: the HTTP/WS API, block following, state, VM, and storage. The staged plan is in [`TODO.md`](TODO.md). Every endpoint starts as a proxy to the official node and becomes native only after it matches the official responses on live traffic.

Until then, `nixiaproject` keeps using the official node directly.

## Run

```bash
cargo run --release -p alephium-app -- --peer 127.0.0.1:9973 --watch
```

The default peer is the isolated interoperability port `127.0.0.1:19985`. Production deployments should set the peer explicitly.

## Read next

- [`docs/NIXIA_API_SURFACE.md`](docs/NIXIA_API_SURFACE.md) — every endpoint and subscription the bot uses, and what each one needs.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — current and target module layout.
- [`TODO.md`](TODO.md) — staged roadmap with exit gates.
- [`AGENTS.md`](AGENTS.md) — scope and correctness rules.

## Quality gates

```bash
make check
make official-interop-fastpath   # for P2P changes
```

Compatibility is tested against the pinned official Alephium implementation recorded in `UPSTREAM.toml`.
