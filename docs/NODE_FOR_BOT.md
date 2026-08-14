# Node Purpose: Fast-Path for `alphrustalbitrage`

## Decision

This repository exists only to accelerate `soulredcat/alphrustalbitrage`.

It is not an independent Alephium client and is not intended to replace the official node. The official node remains mandatory in the production topology.

## Why this component exists

The official node optimizes for correctness and general node operation. The arbitrage bot needs a narrower path:

```text
peer announcement
-> request transaction immediately
-> deliver raw bytes locally
-> identify affected routes
```

The Rust process removes avoidable work from that speculative path:

- no REST request;
- no API DTO conversion;
- no JSON WebSocket payload;
- no database query;
- no full-node synchronization;
- no VM execution;
- no canonical state mutation.

## Production topology

```text
                     +---------------------------+
Alephium network --->| Rust bot fast-path peer   |
                     +-------------+-------------+
                                   |
                                   | ALFP binary events
                                   v
                     +---------------------------+
                     | alphrustalbitrage         |
                     | speculative preparation   |
                     +-------------+-------------+
                                   ^
                                   | validated mempool/block state,
                                   | build, simulation, submit, status
                     +-------------+-------------+
                     | Official Alephium node    |
                     +---------------------------+
```

## Source-of-truth policy

| Data or action | Authority |
| --- | --- |
| First-seen raw transaction | Rust fast-path, speculative |
| Valid local mempool transaction | Official node |
| Pool and contract state | Official node plus bot memory cache |
| Transaction construction | Official node, checked by local policy |
| Simulation/preflight | Official node |
| Signing | Bot signer after local intent verification |
| Submission | Official node initially |
| Confirmation/finality | Official node |

## Event guarantees

The event stream guarantees only:

- frame boundaries;
- protocol version;
- process-local ordering through `sequence`;
- the time this process observed an event;
- raw bytes as received from a peer.

It does not guarantee:

- validity;
- uniqueness across process restarts;
- canonical inclusion;
- successful execution;
- completeness when the consumer disconnects or falls behind.

The consumer must combine `(process epoch, sequence)` once epoch support is added. In protocol version 1, a reconnect to a restarted process must be treated as a new stream even if sequence begins again.

## Security rules

1. IPC must bind to loopback only. The implementation rejects non-loopback addresses.
2. No wallet private key may be loaded by this repository.
3. Raw events must never bypass the official-node execution gate.
4. Event payloads must never contain bot ranking, balances, private keys, or profit policy.
5. Peer-controlled lengths, queues, retries, and in-flight requests must remain bounded.
6. A bot-detected sequence gap must disable speculative-only execution until official-node recovery has caught up.

## Recommended production settings

Start conservatively:

```text
max_inflight_tx_requests = 8
tx_request_batch_size    = 1
tx_request_timeout       = 250 ms
max_tx_request_retries   = 2
event_queue_capacity     = 4096
```

Tune only from measured p50/p95/p99 latency and peer behavior. Lower timeouts can amplify traffic and cause false retries.

## Definition of done

This component is production-ready for the bot only after:

- official interoperability is green;
- event protocol tests are green;
- multi-peer fan-in is implemented;
- bot shadow mode proves no unsafe execution from speculative events;
- sequence-gap recovery is implemented;
- overload tests prove bounded memory;
- reconnect and malformed-payload tests pass;
- 24–72 hour soak testing shows stable latency and no unexplained event loss;
- real-funds execution still requires official-node acceptance and preflight.
