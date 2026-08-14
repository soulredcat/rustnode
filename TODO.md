# Ordered Fast-Path Roadmap

This roadmap is bot-only. Full-node parity, consensus, VM, wallet, mining, and historical synchronization are permanently out of scope unless the owner explicitly changes the repository mission.

## Current pointer

```text
F0 — Scope cut and bounded single-peer fast path
Status: implementation/verification
```

## F0 — Scope cut and single-peer MVP

- [x] Replace the full-node mission with the bot fast-path mission.
- [x] Remove full-node runtime behavior and synchronous disk state from the hot path.
- [x] Keep official Alephium framing, handshake, and transaction payload compatibility.
- [x] Add bounded transaction announcement deduplication.
- [x] Add multiple in-flight `TxsRequest` tracking.
- [x] Add timeout, bounded retry, and exhausted-request release.
- [x] Add versioned loopback binary event delivery.
- [ ] Restore green formatting, compile, Clippy, and tests on CI.
- [ ] Update official interoperability assertions for the reduced runtime.

### F0 exit gate

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
make official-interop-fastpath
```

## F1 — Bot shadow integration

- [ ] Add `infrastructure/fastpath` client modules in `alphrustalbitrage`.
- [ ] Decode ALFP v1 with strict size/version checks.
- [ ] Add process restart and sequence-gap guard.
- [ ] Feed `RawTransaction` into the existing script decoder in metrics-only mode.
- [ ] Correlate Rust event transaction IDs with official-node WebSocket IDs.
- [ ] Record monotonic end-to-end race timestamps.

### F1 exit gate

- No execution decision changes.
- Raw transaction ID/decode comparison is deterministic.
- Every reconnect or sequence gap resets speculative state.

## F2 — Speculative affected-route preparation

- [ ] Build transaction-local affected-pool observations from Rust events.
- [ ] Apply a speculative virtual reserve overlay with TTL.
- [ ] Run affected-route discovery and sizing from memory.
- [ ] Cache prepared opportunities by transaction ID.
- [ ] Require official-node acceptance before build/preflight/submit.
- [ ] Invalidate mismatched or expired speculative results.

### F2 exit gate

- Shadow opportunities match official-source opportunities within documented rules.
- No speculative-only transaction can reach signing.

## F3 — Multi-peer acquisition

- [ ] Replace the single peer with a bounded peer manager.
- [ ] Maintain independent session/request state per peer.
- [ ] Deduplicate announcements globally.
- [ ] Add latency, success, timeout, and malformed-response peer scores.
- [ ] Add hedged fetch after a measured delay.
- [ ] Bound aggregate traffic and memory.

### F3 exit gate

- One slow or malicious peer cannot block other peers.
- Multi-peer p99 beats the best stable single-peer baseline.

## F4 — Event reliability and observability

- [ ] Add process epoch to ALFP protocol v2.
- [ ] Add explicit overflow/gap events.
- [ ] Add heartbeat and health snapshot events.
- [ ] Export Prometheus-compatible counters/histograms outside the hot path.
- [ ] Add reconnect, overload, malformed-frame, and slow-consumer tests.
- [ ] Complete a 24–72 hour soak test.

## F5 — Submission propagation acceleration

- [ ] Accept only already signed transactions that the official node has accepted locally.
- [ ] Verify transaction ID before propagation.
- [ ] Announce/relay through selected peers immediately.
- [ ] Keep official-node submit/status as the canonical workflow.
- [ ] Add strict rate, size, and ownership policy.

## F6 — Production release gate

- [ ] Security review of P2P and IPC inputs.
- [ ] Bot signer policy remains independent.
- [ ] Release build provenance and rollback procedure.
- [ ] Measured p50/p95/p99 improvement documented.
- [ ] No unresolved sequence-gap, memory-growth, or retry-storm issue.
