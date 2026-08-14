# AGENTS.md

## Mission

This repository builds a minimal Alephium transaction fast-path for `soulredcat/alphrustalbitrage`.

It is not a full node. Do not reintroduce the former M3–M8 full-node roadmap.

## Read order

1. `README.md`
2. `ARCHITECTURE.md`
3. `docs/NODE_FOR_BOT.md`
4. `docs/BOT_INTEGRATION.md`
5. `TODO.md`
6. relevant official Alephium source pinned by `UPSTREAM.toml`

## Allowed scope

- official P2P framing and handshake compatibility;
- peer lifecycle and liveness;
- `NewTxHashes`, `TxsRequest`, and `TxsResponse` behavior;
- bounded request tracking, retries, dedupe, and peer scoring;
- raw transaction event delivery to a local bot;
- direct propagation of already signed, locally accepted bot transactions;
- latency, reliability, and security testing for those paths.

## Forbidden scope

Do not add:

- full synchronization;
- BlockFlow/canonical chain state;
- consensus or PoW/PoLW validation;
- Ralph/Alphred VM;
- world-state storage;
- wallet or trading keys;
- mining;
- REST/API parity;
- explorer/indexer functionality;
- bot route, ranking, sizing, or profit policy.

## Correctness rules

- The official Alephium implementation remains the wire-format oracle.
- Do not change official message codes, field order, hashing, signature, or framing semantics.
- Bound every peer-controlled size, queue, retry count, and in-flight map.
- Do not panic on peer or IPC input.
- Do not perform filesystem, database, REST, or JSON work in the transaction hot path.
- Do not block P2P ingestion on a bot consumer.
- Raw P2P events are speculative and must never authorize signing.
- No wallet private key may be introduced into this repository.

## Architecture rules

- `protocol`, `serde`, and `crypto` own compatibility primitives.
- `conf` owns configuration and the non-funded P2P identity.
- `flow` owns fast-path session, request tracking, and IPC only.
- `app` owns CLI and process lifecycle only.
- New responsibilities require a focused module; do not rebuild a monolithic `lib.rs`.

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
- byte-for-byte fixture;
- Rust/official interoperability log;
- bounded overload test;
- latency distribution, not one anecdotal timing.

## Completion policy

A task is complete only when its failure modes are tested. A benchmark does not prove protocol correctness, and a successful decode does not prove transaction validity.
