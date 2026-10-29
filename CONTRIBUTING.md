# Contributing

## Scope

Changes must serve `nixiaproject`'s API surface (`docs/NIXIA_API_SURFACE.md`). Features the bot does not use are rejected, and unused code is deleted.

Read `AGENTS.md`, `ARCHITECTURE.md`, and `TODO.md` first.

## Change requirements

Every change must state:

- the endpoint or roadmap stage the change serves;
- affected hot-path stages;
- official Alephium wire behavior consulted;
- memory, latency, and failure impact;
- tests and evidence;
- explicit out-of-scope behavior.

## Code requirements

- preserve exact official wire bytes;
- keep peer-controlled work bounded;
- avoid blocking I/O in the P2P event path;
- use typed errors at external boundaries;
- keep bot domain policy outside this repository;
- keep wallet keys outside this process;
- do not add `unsafe` without a separate reviewed decision;
- do not claim canonical validity from a raw peer event;
- mainnet only, nothing unused, no dead code (`IMMUTABLE.md`).

## Verification

```bash
make check
make mainnet-check
```

## Commit format

```text
<area>: <imperative summary>
```

Examples:

```text
p2p: bound raw transaction response size
mempool: evict oldest raw transaction when full
api: proxy GET /mempool/transactions to the official node
```
