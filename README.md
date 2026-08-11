# Alephium Rust Node

> Experimental, independent Rust implementation of the Alephium protocol.

This repository is intended to become a production-grade Alephium full node written in Rust. The first objective is **protocol compatibility**, not a protocol fork and not a TPS redesign.

The Rust implementation must be able to join the existing Alephium network, exchange the same P2P messages, validate the same blocks and transactions, execute the same VM semantics, and derive the same consensus-critical state as the reference Scala implementation.

## Status

**Phase: architecture / protocol-parity bootstrap.**

There is no production-ready node in this repository yet. Do not use it for mainnet funds, mining, signing, or consensus decisions until the interoperability and historical replay gates defined in this document are complete.

## Reference implementation

Reference client:

- Repository: https://github.com/alephium/alephium
- Baseline commit: `63bcf6998508025f9538f3377629fe9e66df6c53`
- Baseline date: 2026-07-22

The baseline commit is intentionally pinned. Updating the compatibility target must be explicit and accompanied by regenerated test vectors and differential-test results.

## Project goals

1. Implement the Alephium wire protocol and consensus-critical behavior in native Rust.
2. Preserve byte-for-byte compatibility where the protocol defines serialization.
3. Preserve transaction IDs, block hashes, signature validation, gas accounting, VM results, hard-fork behavior, state transitions, and BlockFlow decisions.
4. Sync from genesis and reach the same chain tips/state as the reference client.
5. Interoperate directly with existing Alephium Scala nodes over P2P.
6. Reduce runtime overhead, memory pressure, and latency without changing consensus semantics.
7. Provide a clean Rust foundation for later research into parallel execution, faster propagation, higher shard capacity, and incentivized service-node infrastructure.

## Non-goals for V1

The compatibility release will **not**:

- change BlockFlow consensus rules;
- change PoW/PoLW security assumptions;
- change block time, gas limits, transaction limits, or shard count;
- introduce staking-based consensus;
- change Ralph/Alphred VM semantics;
- introduce service-node rewards;
- require a new database layout compatible with the Scala client's RocksDB files;
- optimize code at the cost of deterministic behavior.

Protocol evolution belongs in a later, separately activated phase after Rust/Scala parity is proven.

## Core rule

```text
same protocol input
      |
      +-------------------+
      |                   |
      v                   v
Scala reference        Rust node
      |                   |
      v                   v
reference result       Rust result
      |                   |
      +--------- == -------+
```

For consensus-critical operations, "close enough" is a failure.

## Planned workspace

```text
rustnode/
├── Cargo.toml
├── crates/
│   ├── alph-primitives/     # hashes, U256, addresses, groups, chain indexes
│   ├── alph-codec/          # consensus/wire serialization and deserialization
│   ├── alph-crypto/         # protocol cryptography
│   ├── alph-protocol/       # blocks, transactions, outputs, scripts, messages
│   ├── alph-vm/             # Alphred VM execution and gas accounting
│   ├── alph-consensus/      # validation, PoW/PoLW rules, weights, hard forks
│   ├── alph-storage/        # storage interfaces and RocksDB implementation
│   ├── alph-blockflow/      # sharded chain state and BlockFlow logic
│   ├── alph-mempool/        # transaction admission and lifecycle
│   ├── alph-p2p/            # peer protocol, discovery, transport, relay
│   ├── alph-sync/           # headers/blocks/state synchronization
│   ├── alph-api/            # HTTP/WebSocket/RPC compatibility layer
│   ├── alph-node/           # node orchestration and lifecycle
│   └── alph-testkit/        # fixtures, Scala oracle adapters, generators
├── bins/
│   ├── alephium-node/
│   └── alph-tool/
├── tests/
│   ├── vectors/
│   ├── differential/
│   ├── interoperability/
│   └── mainnet-replay/
└── ARCHITECTURE.md
```

See [ARCHITECTURE.md](./ARCHITECTURE.md) for module boundaries, dependency rules, execution model, testing strategy, and post-parity protocol research.

## Dependency direction

The core dependency graph is intentionally one-way:

```text
primitives
   |
codec + crypto
   |
protocol
   |
vm + consensus
   |
storage + blockflow
   |
mempool + p2p + sync
   |
api
   |
node
```

Consensus code must not depend on HTTP, dashboards, service-node economics, or operational tooling.

## Async boundary

Rust async is for I/O orchestration, not deterministic VM semantics.

```text
ASYNC / I-O                  DETERMINISTIC CORE
------------------           ---------------------------
P2P sockets                  codec
peer management              hashing
sync scheduling              crypto verification rules
API/RPC                      transaction validation
storage scheduling           block validation
metrics                      VM execution
shutdown                     gas accounting
                             state transition
                             BlockFlow decisions
```

The VM and consensus-critical execution path should remain synchronous and deterministic. Parallelism may be introduced only where result ordering and state merging are formally deterministic.

## Compatibility gates

A Rust node is not considered compatible until it passes all of the following:

- Scala decode -> Rust encode is byte-for-byte identical for consensus/wire objects.
- Transaction IDs are identical.
- Block hashes are identical.
- Signature verification accepts/rejects the same vectors.
- VM return values are identical.
- VM failure classes and consensus-relevant failure ordering are identical.
- Gas consumption is identical.
- World-state transitions are identical.
- Transaction validation decisions are identical.
- Block validation decisions are identical.
- Hard-fork behavior is identical at every activation boundary.
- BlockFlow chain selection/dependencies are identical.
- Rust <-> Scala P2P handshake and message exchange work without adapters.
- Rust can replay the historical chain from genesis without divergence.
- Rust can run as a shadow mainnet node and remain at the same valid network state.

## Differential testing

The Scala client is the initial protocol oracle.

For each fixture:

```text
fixture bytes/state
      |
      +--------------------+
      |                    |
      v                    v
Scala oracle           Rust implementation
      |                    |
      v                    v
canonical output       candidate output
      |                    |
      +--------- diff ------+
```

Golden vectors should cover both valid and invalid inputs. Invalid vectors are essential: rejecting the same malformed transaction for different reasons can expose semantic divergence that later becomes consensus-critical.

## Development roadmap

### Phase 0 - Protocol lock and oracle

- pin the Scala baseline;
- inventory consensus-critical types and serialization rules;
- create golden-vector extraction tooling;
- define canonical error/result representations for differential tests.

### Phase 1 - Primitives, codec, crypto

- integer and byte primitives;
- hashes and IDs;
- address/group/chain-index types;
- exact serialization/deserialization;
- required cryptographic primitives and signatures;
- property and fuzz tests.

### Phase 2 - Protocol model and VM

- transactions and outputs;
- blocks and headers;
- scripts/contracts;
- hard-fork rules;
- VM stack/frame/instructions;
- exact gas accounting;
- stateful/stateless execution parity.

### Phase 3 - Validation, storage, BlockFlow

- transaction validation;
- block/header validation;
- world-state storage;
- chain/header-chain state;
- BlockFlow dependencies, tips, weights, and view updates;
- historical replay tooling.

### Phase 4 - Mempool, P2P, sync, API

- mempool lifecycle;
- Alephium P2P framing and payloads;
- peer discovery and relay;
- block/header synchronization;
- API compatibility required by existing tooling.

### Phase 5 - Mainnet shadow operation

- genesis-to-tip replay;
- long-running Rust/Scala differential state checks;
- fault injection;
- restart/recovery testing;
- storage corruption and partial-write testing;
- P2P interoperability under real network conditions.

### Phase 6 - Performance engineering

Only after parity:

- profile allocation and copy hot paths;
- bounded concurrency and backpressure;
- batch signature verification where semantics permit;
- compact/efficient internal representations;
- RocksDB batching and cache tuning;
- propagation and sync optimization;
- benchmark CPU, RAM, tail latency, replay speed, and block propagation.

Performance claims must be measured against the pinned Scala reference under equivalent hardware and workloads.

### Phase 7 - Protocol research

Protocol-changing work must be isolated from the compatibility client and require explicit network activation. Candidate research areas include:

- deterministic transaction access sets;
- conflict graphs and parallel VM execution;
- compact-block propagation;
- increased shard/group capacity;
- service-node infrastructure and fee-funded rewards.

None of these are V1 requirements.

## Service-node research direction

A later protocol proposal may introduce infrastructure roles that receive a fee-funded reward without receiving consensus voting power.

Current research model:

| Tier | Proposed bond | Proposed maximum slots | Share of service-node reward pool |
|---|---:|---:|---:|
| Tier 1 / Backbone | 1,000,000 ALPH | 128 | 50% |
| Tier 2 / Service | 100,000 ALPH | TBD | 25% |
| Tier 3 / Edge | 10,000 ALPH | TBD | 25% |

Important constraints:

- the bond is an anti-Sybil/service commitment, **not PoS voting power**;
- reward weight should not grow linearly above the tier minimum;
- reward payout is monthly;
- eligibility/performance is based on a rolling 90-day service record;
- useful service must be provable; heartbeat-only uptime is insufficient;
- the exact fraction of total transaction fees assigned to the service-node reward pool is **TBD**;
- the 50/25/25 split applies to that reward pool, not automatically to all network fees;
- this design requires economic simulation, adversarial analysis, and a protocol upgrade before activation.

## Security principles

- Fail closed on malformed or ambiguous data.
- No unsafe code in consensus crates without a documented, reviewed reason.
- No floating-point arithmetic in consensus-critical calculations.
- Integer overflow/underflow behavior must be explicit.
- Hard-fork rules must be centralized and testable.
- Untrusted network input must have bounded allocation and bounded work.
- All queues must be bounded or explicitly justified.
- Database writes that form one logical state transition must be atomic/recoverable.
- Every optimization must preserve deterministic output.
- Fuzz parsers, VM inputs, transaction validation, and P2P framing.

## Contribution rule for consensus code

Any change touching codec, hashing, VM, gas, transaction/block validation, hard-fork rules, or BlockFlow must include at least one of:

- a new differential test against the Scala oracle;
- a new golden vector;
- a historical replay assertion;
- an interoperability test.

A benchmark alone is never sufficient justification for a consensus change.

## License

A project license has not yet been selected for this repository.

The Alephium reference implementation is licensed under LGPL-3.0. Before copying or mechanically porting reference-client source code, this project must establish and follow an explicit licensing/compliance policy. Prefer protocol reimplementation from documented behavior and verified test vectors rather than uncontrolled source translation.

## Disclaimer

This project is currently experimental and unofficial. It is not a replacement for the Alephium reference client until all compatibility, replay, interoperability, and security gates are satisfied.
