# Architecture

## 1. Mission

Build a working Rust implementation of the official Alephium node that can operate on the same network as the existing Scala implementation.

The architecture is deliberately conservative during the rebuild:

```text
Official Alephium implementation
            |
            | reproduce behavior
            v
       Rust implementation
```

The first milestone is network interoperability. Full-node parity follows incrementally.

No protocol optimization or redesign is part of the initial architecture.

## 2. Source of truth

Upstream reference:

- https://github.com/alephium/alephium
- pinned baseline: `63bcf6998508025f9538f3377629fe9e66df6c53`

When Rust behavior and upstream behavior disagree during the initial rebuild, upstream is treated as the reference until the discrepancy is understood and documented.

The baseline should move only intentionally. Upstream upgrades must be reviewed and ported rather than silently inherited.

## 3. Primary success condition

Before attempting to complete the VM, wallet, API, or performance work, the project must produce a Rust binary that can establish and maintain an Alephium P2P session with an official node.

Target topology:

```text
+---------------------------+          +---------------------------+
| Official Alephium Node    |   TCP    | Alephium Rust Node        |
| Scala                     |<-------->| Rust                      |
|                           |   P2P    |                           |
+---------------------------+          +---------------------------+
```

Initial exchange:

```text
TCP connect
    |
    v
Alephium message framing
    |
    v
Hello --------------------->
      <--------------------- Hello
    |
    v
handshake accepted
    |
    v
Ping ---------------------->
      <--------------------- Pong
    |
    v
stable peer connection
```

This is Milestone 1 and should be reached as early as possible.

## 4. Working-first strategy

A full horizontal port of every Scala module before running anything is too risky. The project should instead build a narrow end-to-end path through the official architecture and expand it.

```text
M0 compile
 |
M1 connect/handshake
 |
M2 request real headers/blocks
 |
M3 decode and hash protocol objects
 |
M4 validate chain data / BlockFlow
 |
M5 execute VM and state transitions
 |
M6 persistent full synchronization
 |
M7 API/tooling parity
 |
M8 optimization
```

At each milestone, the Rust binary must remain runnable.

## 5. Upstream module alignment

The upstream SBT build separates responsibilities into modules such as:

```text
macros
util
serde
io
crypto
api
ws
rpc
app
benchmark
flow
json
conf
protocol
ralph
http
wallet
ralphc
tools
integration
```

The Rust workspace should preserve these responsibility boundaries as closely as practical.

Planned layout:

```text
rustnode/
├── Cargo.toml
├── crates/
│   ├── macros/
│   ├── util/
│   ├── serde/
│   ├── io/
│   ├── crypto/
│   ├── protocol/
│   ├── flow/
│   ├── json/
│   ├── conf/
│   ├── api/
│   ├── ws/
│   ├── rpc/
│   ├── http/
│   ├── ralph/
│   ├── wallet/
│   ├── ralphc/
│   ├── app/
│   ├── tools/
│   └── benchmark/
├── integration/
└── tests/
```

Rust language mechanics may differ from Scala, but module ownership must not drift without a concrete reason.

## 6. Minimum crates for first working peer

Not every upstream module is required to reach the first live connection.

The first vertical slice requires roughly:

```text
util
 |
serde ----- crypto
 |            |
 +-----+------+
       |
   protocol
       |
flow/network
       |
     app
```

### `util`

Initial responsibilities:

- byte helpers;
- timestamps;
- bounded collection helpers required by protocol code;
- fixed-size integer/byte types required by handshake structures;
- common error/result primitives where upstream semantics require them.

### `serde`

This is not Serde-the-Rust-framework as a protocol decision. This crate implements **Alephium's official binary serialization rules**.

Initial requirements:

- integer encoding used by official messages;
- fixed bytes;
- vector/product encoding used by handshake objects;
- exact decode error handling required for P2P framing;
- no acceptance of trailing bytes where upstream rejects them.

### `crypto`

Initial requirements are only those needed for a valid official handshake and message validation. Additional official primitives are added as block/transaction functionality comes online.

The handshake signs broker information and the official node verifies that signature against the broker/clique identity.

### `protocol`

Initial implementation priority:

```text
config/
message/
model/
```

The first protocol message set:

- `Hello`;
- `Ping`;
- `Pong`;
- request IDs;
- broker information;
- P2P version handling;
- release/client ID handling;
- outer `Message` framing.

### `flow`

Only the networking subset is initially required:

- TCP connection lifecycle;
- connection read buffer;
- incremental message framing;
- handshake state;
- ping/pong state;
- peer disconnect/error handling.

BlockFlow itself is added later under the same upstream `flow` responsibility.

### `app`

The first binary should do very little:

1. load config;
2. load/create node identity required for the handshake;
3. connect to a configured official peer;
4. complete handshake;
5. run ping/pong;
6. print connection state and errors;
7. shut down cleanly.

That binary is the first real proof that the Rust protocol implementation works.

## 7. Official P2P framing

The official message envelope must be implemented exactly before higher-level payloads.

Conceptually:

```text
+-------------+
| magic bytes |
+-------------+
| checksum    |
+-------------+
| length      |
+-------------+
| payload     |
+-------------+
```

The official implementation verifies network magic bytes, reads checksum and payload length, then decodes the payload. Rust must use the same byte ordering, length semantics, checksum behavior, and failure rules.

The payload itself begins with the official payload code followed by payload-specific bytes.

Initial payload codes required by Milestone 1 are `Hello`, `Ping`, and `Pong`.

Do not invent a Rust-specific envelope.

## 8. Handshake compatibility

The official `Hello` carries:

```text
Hello {
    client_id,
    timestamp,
    broker_info,
    signature,
}
```

The signature is consensus/network identity data, not decorative metadata. Rust must produce a handshake that passes the official verifier.

The official node also derives release compatibility and P2P protocol version from slash-separated `clientId` fields.

A Rust client ID should remain parser-compatible while identifying itself honestly, for example conceptually:

```text
rust-alephium/<compatible-release>/<os>/p2p-v2
```

Do not claim support for an upstream release/P2P version until the corresponding protocol behavior has been implemented.

### Handshake state machine

```text
Disconnected
    |
    v
Connecting
    |
    v
TCP Connected
    |
    v
Send Hello
    |
    +-----------------------+
    |                       |
receive valid Hello      timeout/error
    |                       |
    v                       v
Established             Disconnect
    |
    v
Ping/Pong + protocol exchange
```

No sync message should be processed as an established peer until the official handshake requirements have succeeded.

## 9. Milestone 2: real protocol data

After handshake/ping-pong works, add official payloads in the order needed to obtain real network data.

Priority:

1. chain state / inventory exchange;
2. header request/response;
3. block request/response;
4. new header/block announcements;
5. transaction hash/request/response messages.

At this stage Rust does not need to pretend that all downloaded data is fully validated. It must clearly distinguish:

```text
decoded
!=
validated
!=
committed to canonical state
```

This prevents an incomplete validator from accidentally being treated as a production full node.

## 10. Milestone 3: protocol model parity

Implement official protocol/model types required by real blocks and transactions.

Priority includes:

- hashes and IDs;
- group and chain indexes;
- addresses and lock/unlock scripts;
- transaction inputs/outputs;
- tokens/assets;
- block dependencies;
- block headers;
- blocks;
- hard-fork/version types.

### Golden rule

For every protocol model with canonical bytes:

```text
Scala serialize(x) == Rust serialize(x)
```

and:

```text
Rust deserialize(Scala serialize(x)) == x
```

for supported vectors.

## 11. Milestone 4: official validation and BlockFlow

Only after real protocol objects are decoded correctly should Rust begin reproducing official validation.

Responsibilities include:

- header validation;
- block validation;
- transaction validation;
- PoW/PoLW-related protocol checks;
- weight calculation;
- dependencies;
- chain/group rules;
- hard-fork-dependent validation;
- BlockFlow views/tips/state decisions.

This work should follow the official `protocol` and `flow` implementations closely.

No attempt should be made to create a better consensus algorithm during this phase.

## 12. Milestone 5: Ralph/Alphred VM and world state

VM parity is consensus-critical.

Rust must reproduce:

- values;
- instructions;
- stack behavior;
- frames;
- stateless/stateful execution;
- contract calls;
- asset handling;
- failure behavior;
- gas accounting;
- hard-fork-specific instruction behavior;
- resulting state transitions.

A faster but semantically different VM is invalid.

Differential tests against the official implementation are mandatory here.

## 13. Milestone 6: storage and full sync

The physical Rust database layout does not have to match Scala RocksDB files unless interoperability requirements later demand it.

The **logical stored data and reconstructed state must match**.

Required properties:

- persistent headers/blocks/state;
- atomic/recoverable logical updates;
- restart without consensus divergence;
- synchronization from genesis;
- resumption after interrupted sync;
- same accepted historical chain as official node;
- same chain tips after replay.

At the end of this milestone, Rust should behave as a real full node, not only a P2P client.

## 14. API, wallet and compiler

These are implemented after the core node works unless a specific API endpoint is required for integration testing.

Priority:

```text
working network node
>
full validation/state
>
full synchronization
>
API compatibility
>
wallet/ralphc/tools parity
```

This prevents peripheral tooling from delaying the core objective.

## 15. Differential testing architecture

The official node acts as an oracle during development.

```text
                   fixture
                     |
              +------+------+
              |             |
              v             v
        Scala reference   Rust
              |             |
              v             v
            result        result
              |             |
              +------==-----+
```

Differential coverage should include:

### Wire

- Message envelope encode/decode;
- every payload encoding;
- malformed frames;
- wrong network magic;
- invalid/truncated lengths;
- invalid payload codes.

### Crypto

- known signatures;
- invalid signatures;
- hashes;
- key/address derivation where protocol-visible.

### Protocol

- transaction bytes/IDs;
- block/header bytes/hashes;
- valid and invalid structures.

### Validation

- accepted/rejected transaction vectors;
- accepted/rejected header vectors;
- accepted/rejected block vectors;
- hard-fork boundary cases.

### VM

- result values;
- errors;
- gas;
- contract state;
- transaction outputs.

### Full node

- historical replay;
- Rust <-> Scala P2P integration;
- restart/recovery;
- tip/state comparison.

## 16. Error handling

Network input is untrusted.

The Rust implementation must:

- fail closed on malformed protocol bytes;
- bound declared payload sizes before allocation;
- reject invalid lengths/checksums/magic bytes;
- avoid panic on peer-controlled data;
- make protocol errors distinguishable from local I/O/storage failures;
- disconnect according to official behavior where required.

Consensus code should use typed errors rather than strings as internal control flow, even if external/log representations mimic upstream behavior.

## 17. Concurrency

During the initial rebuild, concurrency should be chosen for correctness and simplicity.

Do not parallelize consensus-critical work merely because Rust makes it possible.

The first networking implementation may use Tokio for TCP and timers, while deterministic protocol/validation/VM functions should remain ordinary synchronous functions unless upstream semantics require otherwise.

```text
Tokio / async
    |
network + timers + orchestration
    |
    v
synchronous deterministic protocol core
```

Optimization of scheduling, lock contention, allocations, batching, parallel validation, or VM execution is a later phase.

## 18. Dependency discipline

Expected conceptual dependency direction:

```text
util
 |
serde     crypto
  \       /
   protocol
      |
     conf
      |
  io / flow
      |
 api/ws/rpc/http
      |
     app
```

The exact Cargo graph may need small differences because Rust cannot reproduce Scala/SBT mechanics literally. However circular responsibilities should not be introduced simply to make the port easier.

## 19. Definition of done for the initial rebuild

The initial Rust rebuild is complete only when all of the following are true:

- Rust node can connect directly to official Alephium peers;
- official peers accept its handshake;
- P2P messages are wire-compatible;
- Rust can sync from genesis;
- Rust validates the same canonical chain;
- transaction/block hashes match official behavior;
- VM results and gas accounting match;
- hard-fork behavior matches;
- persistent restart/recovery works;
- chain tips/state remain consistent with an official node;
- required node API behavior is compatible.

Only after these gates are satisfied does the project enter an optimization phase.

## 20. Optimization policy

Optimization is intentionally outside the initial implementation scope.

After parity:

1. establish equivalent benchmarks against the official node;
2. profile CPU, memory, storage and network hot paths;
3. optimize implementation details without changing protocol behavior;
4. keep differential and replay tests as regression gates;
5. discuss protocol changes in separate proposals/branches only if desired later.

The first question is therefore not:

> How much faster can Rust be?

It is:

> Can the Rust node correctly join and operate on the official Alephium network?

That is the architecture's first priority.
