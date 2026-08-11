# ADR-0001: Compatibility Before Optimization

- **Status:** Accepted
- **Date:** 2026-08-11
- **Decision owners:** project maintainers
- **Applies to:** V1 compatibility implementation

## Context

The project intends to rebuild the official Alephium node in Rust. There are many possible later improvements: lower resource usage, faster networking, parallel validation/execution, additional shard capacity, and fee-funded service-node infrastructure.

Attempting those changes while the Rust implementation is still incomplete would make failures difficult to classify:

- Rust implementation bug;
- misunderstood official behavior;
- intentional protocol difference;
- optimization-induced divergence;
- hard-fork mismatch;
- database or scheduling difference.

A blockchain client must agree exactly on consensus-critical behavior. A client that is faster but derives a different state or rejects/accepts different blocks is not an optimization; it is an incompatible network fork.

## Decision

V1 will reproduce official Alephium behavior before introducing protocol or semantic optimization.

The official Scala implementation at commit:

```text
63bcf6998508025f9538f3377629fe9e66df6c53
```

is the pinned behavioral reference for initial development.

The project will proceed through runnable vertical milestones:

```text
compile
  -> official P2P handshake
  -> real header/block exchange
  -> protocol object and hash parity
  -> validation and BlockFlow parity
  -> VM/state parity
  -> persistent full synchronization
  -> required API parity
  -> measured optimization
```

## Consequences

### Positive

- Divergence has a clear reference result.
- Differential tests can be created early.
- The Rust node can become useful incrementally.
- Official-node interoperability is tested before deep implementation work.
- Performance work has a correct baseline.
- Future protocol proposals remain separable and auditable.

### Negative

- Some non-idiomatic upstream behavior may need to be preserved initially.
- Optimization opportunities may be deliberately postponed.
- Maintaining fixtures and a pinned upstream baseline adds work.
- Full parity is slower than building a partial custom node.

These costs are accepted because correctness is the primary requirement.

## Explicitly deferred

The following are not part of V1:

- altered transaction/block/gas limits;
- changed BlockFlow or PoW/PoLW rules;
- changed Ralph/Alphred VM semantics;
- fee redistribution or service-node rewards;
- ALPH bond/slot tiers;
- new transaction access lists;
- new consensus authority;
- execution ordering changes;
- protocol-level compact blocks;
- increased groups/shards;
- performance claims without equivalent benchmarks.

## What optimization is allowed before V1 parity?

Only implementation-level work that preserves observable behavior and is necessary for basic safety is allowed, for example:

- bounded allocations;
- bounded channels;
- avoiding accidental copies when semantics remain identical;
- clean cancellation and backpressure;
- typed errors;
- safe cryptographic library wrappers verified by official vectors.

Any change that could alter encoded bytes, accepted inputs, validation order, hashes, gas, state transition, chain selection, or network semantics requires parity evidence and must not be justified solely as optimization.

## Exit condition

This decision may be superseded only after the compatibility client passes all applicable gates:

- byte-level codec parity;
- hash/signature parity;
- validation parity;
- VM/gas/state parity;
- BlockFlow parity;
- official P2P interoperability;
- genesis-to-tip replay;
- stable shadow-node operation.

A new ADR must define the exact optimization or protocol research phase and its activation/isolation strategy.