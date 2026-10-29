# rustnode

An Alephium mainnet node in Rust, built only for the `soulredcat/nixiaproject` arbitrage bot. Mainnet is the only network it knows; [`IMMUTABLE.md`](IMMUTABLE.md) holds the rules nothing here may break.

The goal is a node that serves exactly the HTTP and WebSocket endpoints `nixiaproject` calls ([`docs/NIXIA_API_SURFACE.md`](docs/NIXIA_API_SURFACE.md)), with the same responses as the official node, while keeping only the state, blocks, and indexes those endpoints need. The bot switches by changing `ALEPHIUM_NODE_URL` and nothing else; `nixiaproject` itself is never modified from here.

rustnode is meant to become the bot's only node: it follows the chain from Alephium peers on its own, after a one-time bootstrap from the operator's synced official node, and the official node is then shut down. Until rustnode is complete (`TODO.md`, N8), keep the bot on the official node.

Anything the bot does not use is not built, and unused code is deleted.

## Current status

Available today:

- **API gateway** (`crates/api`): serves the 23 endpoints of the bot's surface (HTTP and `/ws`) and forwards each one unchanged to the official node. Anything outside the surface gets `404`/`405`. Measured overhead is about 60 µs at p50 and under 200 µs at p99 on loopback.
- official Alephium frame codec and the payloads the runtime uses (`Hello`, `Ping`/`Pong`, `InvRequest`/`InvResponse`, `ChainState`, `NewTxHashes`, `TxsRequest`/`TxsResponse`), checked against Scala-generated mainnet vectors and the official mainnet node (`make mainnet-check`);
- bounded transaction acquisition (`NewTxHashes` → `TxsRequest`) with dedupe, in-flight limits, timeouts, and retries, into a bounded raw mempool.

- **Native handlers with shadow checking** (N1/N2): `GET /addresses/{address}/group` is computed by rustnode by default for asset addresses (P2PKH, P2MPKH, P2SH), checked against the official `AddressSpec` vectors and against the official mainnet node (`make mainnet-check`). Contract, grouped, and invalid addresses still go to the official node.

Not built yet: the other native handlers, state and its storage, and the VM. The staged plan is in [`TODO.md`](TODO.md). Each endpoint stays proxied until a native handler matches the official responses, so for now rustnode still needs the official node behind it.

## Run

During development the bot stays on the official node. Running rustnode in front of it, as below, is optional: it lets you check rustnode's answers against the bot's live traffic before the final switch.

Start the official node as usual (REST on `127.0.0.1:12973`) with the settings in [`config/official-node/nixia.conf`](config/official-node/nixia.conf) merged into its `user.conf`: WebSocket, the event log, and the indexes the bot reads are all off by default in the official node. Use official v4.7.0 or newer: v4.6.0 sometimes accepts a WebSocket and then never answers on it (fixed upstream in `5c40f665a`), which leaves a subscription silent until the bot's idle timeout reconnects, and v4.7.0 is the version whose OpenAPI has the `mempool` flag the bot sends on `/addresses/{address}/balance`. Then:

```bash
cargo run --release -p alephium-app -- --api-bind 127.0.0.1:22973 --upstream http://127.0.0.1:12973
```

To send the bot through it, change one line in `nixiaproject`'s `.env`:

```dotenv
ALEPHIUM_NODE_URL=http://127.0.0.1:22973
```

`ALEPHIUM_NODE_API_KEY` keeps working: the `X-API-KEY` header is forwarded to the official node, and when the official node refuses a request or a WebSocket, the bot gets the same status and body. If the official node has an `api-key`, give rustnode the same key in its environment, `RUSTNODE_API_KEY=<key>`, so routes in `native` mode check it as the official node does; without it, requests carrying a key are always forwarded. `nixiaproject`'s WebSocket client sends no key, so its `/ws` subscriptions work only when the official node has no `api-key`, with or without rustnode. To roll back, set `ALEPHIUM_NODE_URL` back to `http://127.0.0.1:12973`.

With a peer (`--peer <host:port>`, the official node's P2P port, 9973 on mainnet; add `--watch` to reconnect), rustnode follows the chain itself: it first copies the last `--window` heights of every chain (default 100,000, about 9 days) from the official node's REST API into a file on disk (`--data`, default `.local/rustnode/chain.redb`), then follows new blocks over P2P v2, and serves followed blocks on native routes. Only the newest 1,024 heights of each chain stay in memory; older blocks are read from the file. A restart reopens the file and, when the official node agrees with the stored chain, fetches only the heights missed meanwhile; without the official node it continues from the file and peers bring the rest. It also fetches announced transactions into a raw mempool for the future native `GET /mempool/transactions`. `--follow`, without `--peer`, follows from the six official mainnet bootstrap peers instead; `--peer` is repeatable. `--help` lists every option.

The gateway logs per-endpoint request counts and latency to stderr every 60 seconds.

### Taking an endpoint native

Each route runs in one of three modes, set with `--route-mode <id>=<mode>` (ids are listed by `docs/NIXIA_API_SURFACE.md` and in the startup log):

- `proxy`: forwarded to the official node;
- `shadow` (default for a new native handler): the bot still gets the official answer; rustnode computes its own and logs `shadow_match` / `shadow_mismatch` counts, plus details of the first mismatches;
- `native` (default once a handler has matched the official node on live traffic, as `address-group` has): rustnode answers; anything its handler cannot answer exactly like the official node is still forwarded.

With no flags, everything the bot uses is on: every endpoint and `/ws` is served, and verified handlers answer natively. A new handler runs in `shadow` through a full trading session; when the log shows `shadow_mismatch=0`, it becomes native by default in the next release, or right away with, for example, `--route-mode <id>=native`. `--route-mode address-group=shadow` or `=proxy` rolls a route back.

## Read next

- [`IMMUTABLE.md`](IMMUTABLE.md) — mainnet only, only what is used, dead code deleted, clean build.
- [`docs/NIXIA_API_SURFACE.md`](docs/NIXIA_API_SURFACE.md) — every endpoint and subscription the bot uses, and what each one needs.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — current and target module layout.
- [`TODO.md`](TODO.md) — staged roadmap with exit gates.
- [`AGENTS.md`](AGENTS.md) — scope and correctness rules.

## Quality gates

```bash
make check
make mainnet-check   # P2P and API changes, against the official mainnet node
```

Compatibility is tested against the pinned official Alephium implementation recorded in `UPSTREAM.toml` and the operator's official mainnet node (`docs/MAINNET_CHECK.md`).
