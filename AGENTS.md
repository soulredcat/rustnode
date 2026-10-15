# AGENTS.md

## Mission

`rustnode` is an Alephium node built only for the `soulredcat/nixiaproject` arbitrage bot.

It serves exactly the HTTP and WebSocket surface that `nixiaproject` uses, listed in `docs/NIXIA_API_SURFACE.md`, with the same paths, parameters, and JSON shapes as the official node, optimized and pruned for that bot. Switching the bot over must require only changing its `ALEPHIUM_NODE_URL`.

rustnode is the bot's permanent node: once it is complete, the operator shuts the official node down. It is independent of the official node at run time and follows the chain from Alephium peers itself. The operator's synced official node is used only for a one-time bootstrap (state, recent blocks, permanent indexes, verified against `depStateHash`) and, during development, as the oracle for differential checks. Until rustnode is complete, the bot keeps using the official node directly; the upstream proxy is a transition and verification tool, removed at N8.

`nixiaproject` is read-only for this repository. Never require a change there.

## Read order

1. `README.md`
2. `docs/NIXIA_API_SURFACE.md`
3. `ARCHITECTURE.md`
4. `TODO.md`
5. relevant official Alephium source pinned by `UPSTREAM.toml`

## Scope rule

Only features `nixiaproject` uses. If nothing on the API surface needs it, it does not belong here; delete unused code rather than keeping it for later. Git history keeps it.

In scope, as the roadmap reaches them:

- official P2P framing, handshake, liveness, transaction and block acquisition;
- following the chain from a verified starting point, with header PoW checks;
- world state and Ralph VM execution needed for the endpoints on the surface;
- bootstrapping state from an operator's fully synced official node data, verified against block `depStateHash`;
- the HTTP and `/ws` endpoints in `docs/NIXIA_API_SURFACE.md`, and, until N8, an upstream proxy to the official node for endpoints not yet served natively;
- pruned storage as listed in the retention table of `docs/NIXIA_API_SURFACE.md`: current state plus 128 heights for reorgs and pinned reads, recent blocks and events in a configured window (default 10,000 heights per chain), and the few permanent exceptions the bot needs.

Out of scope:

- any endpoint or subscription `nixiaproject` does not call;
- mining, wallet APIs, or any private key with funds;
- explorer/indexer functionality beyond the indexes the surface needs;
- full history retention;
- bot route, ranking, sizing, or profit policy.

## Correctness rules

- The official Alephium implementation pinned in `UPSTREAM.toml` is the oracle for wire format, state transitions, and API responses.
- Do not change official message codes, field order, hashing, signature, or framing semantics.
- An endpoint is served natively only after it matches the official node on live traffic; until then it is proxied.
- Stop serving state (fail closed) when a computed state hash diverges from the chain's `depStateHash`.
- Bound every peer-controlled size, queue, retry count, and in-flight map.
- Do not panic on peer or API input.
- No wallet private key may be introduced into this repository.

## Architecture rules

- `protocol`, `serde`, and `crypto` own compatibility primitives, limited to what the runtime uses.
- `model` owns the protocol data types (blocks, transactions, scripts, values) and their official encoding and hashes.
- `conf` owns configuration and the non-funded P2P identity.
- `flow` owns the P2P session, request tracking, and the raw mempool.
- `api` owns the HTTP and `/ws` gateway: the allowlist, the upstream proxy, and native handlers as they arrive.
- `app` owns CLI and process lifecycle only.
- New responsibilities (API server, storage, state, VM) get their own focused crate; do not rebuild a monolithic `lib.rs`.

## Verification

Run:

```bash
make check
```

For P2P changes, also run:

```bash
make official-interop-fastpath
```

Required evidence depends on the change:

- official Scala source/test reference;
- byte-for-byte fixture from the official implementation;
- Rust/official interoperability log;
- differential comparison of API responses against the official node;
- bounded overload test;
- latency and memory measurements, not one anecdotal timing.

## Completion policy

A task is complete only when its failure modes are tested. A benchmark does not prove protocol correctness, and a successful decode does not prove transaction validity.
