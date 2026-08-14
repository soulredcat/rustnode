# Contributing

## Scope

Changes must improve the Alephium transaction fast path used by `alphrustalbitrage`. Full-node features are rejected by design.

Read `AGENTS.md`, `ARCHITECTURE.md`, and `TODO.md` first.

## Change requirements

Every change must state:

- the fast-path problem being solved;
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
- do not claim canonical validity from a raw peer event.

## Verification

```bash
make check
make official-interop-fastpath
```

## Commit format

```text
<area>: <imperative summary>
```

Examples:

```text
fastpath: queue announcements while requests are in flight
ipc: reject non-loopback bind addresses
p2p: bound raw transaction response size
```
