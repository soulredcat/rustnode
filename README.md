# rustnode

An Alephium node in Rust, built only for the `soulredcat/nixiaproject` arbitrage bot.

The goal is a node that serves exactly the HTTP and WebSocket endpoints `nixiaproject` calls ([`docs/NIXIA_API_SURFACE.md`](docs/NIXIA_API_SURFACE.md)), with the same responses as the official node, while keeping only the state, blocks, and indexes those endpoints need. The bot switches by changing `ALEPHIUM_NODE_URL` and nothing else; `nixiaproject` itself is never modified from here.

Anything the bot does not use is not built, and unused code is deleted.

## Current status

Available today:

- **API gateway** (`crates/api`): serves the 23 endpoints of the bot's surface (HTTP and `/ws`) and forwards each one unchanged to the official node. Anything outside the surface gets `404`/`405`. Measured overhead is about 60 µs at p50 and under 200 µs at p99 on loopback.
- official Alephium frame codec and the payloads the runtime uses (`Hello`, `Ping`/`Pong`, `InvRequest`/`InvResponse`, `ChainState`, `NewTxHashes`, `TxsRequest`/`TxsResponse`), checked against Scala-generated vectors and a live official node in CI;
- bounded transaction acquisition (`NewTxHashes` → `TxsRequest`) with dedupe, in-flight limits, timeouts, and retries, into a bounded raw mempool.

- **Native handlers with shadow checking** (N1/N2): `GET /addresses/{address}/group` is computed by rustnode for asset addresses (P2PKH, P2MPKH, P2SH), checked against the official `AddressSpec` vectors and against the real official node in CI. Contract, grouped, and invalid addresses still go to the official node.

Not built yet: the other native handlers, block following, state, VM, and storage. The staged plan is in [`TODO.md`](TODO.md). Each endpoint stays proxied until a native handler matches the official responses on live traffic, so the official node is still required behind the gateway.

## Run

Start the official node as usual (REST on `127.0.0.1:12973`), then:

```bash
cargo run --release -p alephium-app -- --api-bind 127.0.0.1:22973 --upstream http://127.0.0.1:12973
```

Point the bot at it by changing one line in `nixiaproject`'s `.env`:

```dotenv
ALEPHIUM_NODE_URL=http://127.0.0.1:22973
```

`ALEPHIUM_NODE_API_KEY` keeps working: the `X-API-KEY` header is forwarded to the official node. To roll back, set `ALEPHIUM_NODE_URL` back to `http://127.0.0.1:12973`.

P2P transaction acquisition runs only when a peer is given (`--peer <host:port>`, add `--watch` to reconnect). It feeds the raw mempool for the future native `GET /mempool/transactions` and is not needed by the gateway. `--help` lists every option.

The gateway logs per-endpoint request counts and latency to stderr every 60 seconds.

### Taking an endpoint native

Each route runs in one of three modes, set with `--route-mode <id>=<mode>` (ids are listed by `docs/NIXIA_API_SURFACE.md` and in the startup log):

- `proxy`: forwarded to the official node;
- `shadow` (default for routes with a native handler): the bot still gets the official answer; rustnode computes its own and logs `shadow_match` / `shadow_mismatch` counts, plus details of the first mismatches;
- `native`: rustnode answers; anything its handler cannot answer exactly like the official node is still forwarded.

Run in `shadow` through a full trading session. When the log shows `shadow_mismatch=0`, switch that route with, for example, `--route-mode address-group=native`. Switching back is the same flag with `shadow` or `proxy`.

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
