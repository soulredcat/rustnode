# Alephium Bot Fast-Path

A minimal Rust P2P sidecar for `soulredcat/alphrustalbitrage`.

This repository is **not an Alephium full-node replacement**. Its only production purpose is to reduce transaction-observation latency for the arbitrage bot while the official Alephium node remains the authority for validation, state, transaction construction, simulation, submission, and confirmation.

## Responsibilities

The process:

1. connects to an Alephium P2P peer using the official wire protocol;
2. receives `NewTxHashes` announcements;
3. immediately issues bounded concurrent `TxsRequest` requests;
4. receives raw transaction bytes;
5. emits versioned binary events over loopback TCP to the bot.

It deliberately does **not** implement:

- full historical synchronization;
- canonical BlockFlow or world state;
- transaction, block, PoW, or PoLW validation;
- Ralph/Alphred VM execution;
- wallet, mining, REST, explorer, or indexer APIs;
- signing or custody of trading keys.

## Current status

Available:

- official Alephium frame and payload codecs required by the fast path;
- signed `Hello` interoperability and `Ping/Pong` liveness;
- transaction announcement decoding;
- bounded pending queue;
- multiple in-flight transaction requests;
- timeout/retry handling;
- deduplication;
- loopback-only binary event publisher.

Still required before real-funds production use:

- multi-peer fan-in and peer scoring;
- shadow latency comparison against the official node WebSocket;
- integration in `alphrustalbitrage`;
- soak, overload, reconnect, and event-gap testing;
- direct transaction propagation after official-node acceptance.

## Run

```bash
cargo run --release -p alephium-app -- \
  --peer 127.0.0.1:9973 \
  --ipc-bind 127.0.0.1:19090 \
  --watch
```

The default peer remains the isolated interoperability port `127.0.0.1:19985`. Production deployments should set the peer explicitly.

## Bot integration

Read:

- [`docs/NODE_FOR_BOT.md`](docs/NODE_FOR_BOT.md) — scope, trust boundaries, event protocol, and deployment model.
- [`docs/BOT_INTEGRATION.md`](docs/BOT_INTEGRATION.md) — exact steps for `alphrustalbitrage` to consume events.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — module boundaries and runtime data flow.
- [`TODO.md`](TODO.md) — ordered bot-only roadmap.

## Quality gates

```bash
make check
```

Wire compatibility must continue to be tested against the pinned official Alephium implementation recorded in `UPSTREAM.toml`.

## Safety

Raw P2P transaction events are speculative. The bot may use them to precompute affected routes, but must not authorize signing or submission solely from these events. The official local node remains the execution gate.
