# Official-Node Interoperability

## Goal

Prove that the Rust implementation speaks the official Alephium protocol directly and is accepted by an unmodified official Scala node.

Interoperability is not inferred from similar structures or successful Rust-only tests. It requires evidence from both implementations.

Pinned reference:

```text
https://github.com/alephium/alephium
63bcf6998508025f9538f3377629fe9e66df6c53
```

## Non-negotiable test rule

The topology must be:

```text
Official Scala node <---- raw Alephium TCP/P2P ----> Rust node
```

Forbidden in compatibility tests:

- protocol-rewriting proxies;
- gateways that translate JSON to P2P;
- modified official-node validation;
- skipped signature checks;
- hardcoded acceptance of the Rust peer;
- capturing one official packet and replaying it as if the Rust implementation generated it;
- declaring success after TCP connect without a valid Hello exchange.

A packet capture may observe traffic, but it must not modify it.

## Test environments

Use environments in this order:

1. **Deterministic scripted peer tests** inside the Rust workspace.
2. **Local isolated official node** using official integration/devnet configuration.
3. **Official testnet shadow peer** after local tests pass.
4. **Mainnet shadow peer** only after safety review; no mining, signing, or funds required.

Do not begin interoperability development against random public mainnet peers. Local tests provide controlled logs, versions, identity, failure injection, and repeatability.

## Environment manifest

Every interoperability run must record:

```text
run_id:
date_utc:
official_repository:
official_commit_or_release:
official_build_command:
official_start_command:
rust_commit:
rust_build_command:
rust_start_command:
network_id:
p2p_version:
official_listen_address:
rust_local_address:
configuration_files:
test_duration:
expected_result:
actual_result:
log_artifacts:
packet_capture_artifact_if_any:
operator_notes:
```

Never commit private keys, seed phrases, production API keys, or unsanitized sensitive configuration.

## Milestone 1 protocol path

Target sequence:

```text
Rust                               Official node
 |                                      |
 |------------ TCP connect ----------->|
 |                                      |
 |------------- Hello ---------------->|
 |<------------ Hello -----------------|
 |                                      |
 |       both Hello messages valid      |
 |       compatible release/P2P         |
 |       broker identity accepted       |
 |                                      |
 |------------- Ping ----------------->|
 |<------------ Pong ------------------|
 |                                      |
 |<------------ Ping ------------------|
 |------------- Pong ----------------->|
 |                                      |
 |---------- stable session ---------->|
```

The exact order of the two Hello sends may depend on connection scheduling, but the state machine must permit only the official handshake behavior and must validate the remote Hello before entering the active state.

## M1 wire specification checklist

No implementation begins until every unchecked item has an upstream path, exact type, and fixture.

### Message envelope

- [ ] network-specific magic bytes;
- [ ] checksum type width;
- [ ] exact checksum preimage;
- [ ] checksum algorithm;
- [ ] length width;
- [ ] length byte order;
- [ ] maximum accepted frame size;
- [ ] empty payload policy;
- [ ] trailing-frame handling;
- [ ] malformed/incomplete frame behavior.

Known envelope shape at the pinned baseline:

```text
magic bytes | checksum | 4-byte length | payload bytes
```

### Payload code

The official payload code is encoded before payload-specific data. Numerical codes are positional and must follow the complete official order, even if Rust initially implements only:

```text
Hello
Ping
Pong
```

Known but not-yet-implemented official codes must produce an explicit bounded error/dispatch result. They must not be renumbered.

### Hello

Official field order:

```text
client_id
Timestamp
InterBrokerInfo
Signature
```

Validation checklist:

- [ ] client ID string encoding;
- [ ] client ID maximum length of 256 bytes/characters as defined upstream;
- [ ] release version extracted from the correct slash-separated field;
- [ ] supported P2P suffix;
- [ ] active-network minimum version policy;
- [ ] timestamp encoding and unit;
- [ ] timestamp greater than zero;
- [ ] `InterBrokerInfo` field order and validation;
- [ ] clique/broker identity encoding;
- [ ] broker-info hash construction;
- [ ] exact signature schema;
- [ ] exact signed bytes (`brokerInfo.hash.bytes` at the pinned baseline);
- [ ] malformed signature/public-key behavior;
- [ ] redundant trailing-byte behavior.

### Ping/Pong

Checklist:

- [ ] request ID type/width/encoding;
- [ ] Ping timestamp type/encoding;
- [ ] Pong contains the matching request ID;
- [ ] unsolicited/stale Pong policy;
- [ ] ping interval;
- [ ] response timeout;
- [ ] disconnect/misbehavior policy;
- [ ] timer cancellation on connection shutdown.

## Fixture-generation strategy

### Why fixtures are required

Rust encode -> Rust decode tests can pass when both functions implement the same wrong format. Every M1 type needs official output.

### Required fixture groups

```text
tests/fixtures/m1/
  message-envelope/
  release-version/
  p2p-version/
  broker-info/
  hello/
  ping/
  pong/
  invalid/
```

Each fixture must include the metadata required by M0-003.

### Official fixture generator

Add a small, isolated generator under a clearly named tooling directory or maintain a reproducible patch/script against the official repository. It should produce canonical machine-readable output for:

- serialized object bytes;
- object field values;
- relevant hash/preimage;
- validation result;
- failure category for invalid cases.

Do not copy generated random keys without storing the deterministic seed or explicit key bytes used by the fixture.

### Invalid vectors

At minimum generate or construct independently reviewable cases for:

- wrong magic bytes;
- truncated checksum;
- truncated length;
- negative/invalid length;
- oversized length;
- wrong checksum;
- unknown payload code;
- redundant payload bytes;
- malformed client ID;
- unsupported P2P version;
- obsolete release version where applicable;
- zero timestamp;
- invalid broker/group identity;
- malformed public key;
- malformed signature;
- signature for different broker info;
- Ping/Pong request ID mismatch.

## Scripted peer test matrix

Before launching the official node, the Rust test suite must simulate both initiator and responder paths.

| Case | Expected result |
|---|---|
| valid complete Hello in one read | transition to active |
| valid Hello split byte-by-byte | transition to active |
| two frames in one read | preserve and dispatch both |
| wrong network magic | close with protocol error |
| checksum mismatch | close with protocol error |
| oversized frame length | reject before allocation |
| Ping before valid Hello | reject/close according to upstream state behavior |
| duplicate Hello after activation | reproduce upstream behavior |
| Hello timeout | close and cancel timers |
| invalid client ID | close with compatibility reason |
| invalid signature | close with validation reason |
| remote closes mid-frame | no panic; clean incomplete-frame result |
| local shutdown during read | tasks terminate cleanly |
| local shutdown during write | tasks terminate cleanly |
| Ping received when active | send matching Pong |
| Pong with unknown request ID | reproduce upstream policy |

## Local official-node procedure

The exact build and configuration commands must be verified against the pinned official repository and then filled into the run manifest. Do not guess configuration keys.

General procedure:

1. Check out the pinned upstream commit.
2. Build the official node using its documented build process.
3. Create an isolated data directory.
4. Select the official integration/devnet network configuration.
5. Configure a deterministic P2P listen address and any required broker/group identity.
6. Disable unrelated public exposure where possible.
7. Start the official node and confirm it is ready to accept P2P connections.
8. Start the Rust node with matching network and P2P settings.
9. Capture structured logs from both processes.
10. Assert the handshake and liveness conditions below.
11. Shut both processes down cleanly.
12. Preserve sanitized evidence and update the worklog.

The final automated test should orchestrate these steps without changing official-node protocol validation.
`make official-interop-m1` provides a minimal harness with marker checks and reproducible
run metadata in `tests/interoperability/m1/commands.txt`.

## M1 assertions

### Rust-side assertions

- TCP connect succeeds;
- local Hello was generated by Rust code;
- official Hello frame decoded and validated;
- connection reached `Active` only after valid Hello;
- negotiated P2P version is recorded;
- at least one Ping/Pong cycle in each direction succeeds;
- no unbounded queue or buffer growth;
- shutdown and reconnect succeed.

### Official-node assertions

Use official logs/metrics/tests to establish:

- Rust peer Hello was accepted;
- client/release/P2P version was accepted;
- broker identity/signature was accepted;
- official node did not mark the Rust peer as invalid or misbehaving;
- official node dispatched active-session messages;
- official node accepted/responded to Ping/Pong;
- connection remained stable for the soak period.

### Exit criteria

M1-011 can be marked `DONE` only when all required assertions are reproducible from a clean checkout.

## Soak and reconnect tests

Use two durations:

- short automated test suitable for routine CI;
- longer recorded soak test for milestone acceptance.

The exact duration should be recorded in task/config rather than hardcoded throughout the code.

Test:

- repeated liveness cycles;
- official-node restart;
- Rust-node restart;
- network interruption;
- reconnect backoff;
- cancellation during backoff;
- no duplicate long-lived peer tasks after reconnect.

## P2P V1 and V2

The pinned official implementation recognizes P2P V1 and V2 from client ID structure. The active official configuration determines which path is selected.

Implementation order:

1. identify the version enabled in the controlled official test configuration;
2. make one version fully interoperable;
3. add the second version as a separate task/test if behavior differs;
4. never claim generic P2P support based on only one unrecorded path.

Every run manifest must state the negotiated P2P version.

## Security requirements for the test harness

- Bind test services to loopback/private test interfaces unless external testing is intentional.
- Generate dedicated test identities.
- Never reuse production wallet/miner keys.
- Bound logs and packet captures.
- Sanitize secrets before committing artifacts.
- Use unique data directories per run.
- Kill child processes on test failure and timeout.
- Detect port conflicts instead of connecting to an unintended process.
- Verify the official process commit/release before accepting test results.

## Failure-report format

When interoperability fails, record:

```text
first divergent stage:
last accepted frame:
raw frame hash/fixture reference:
Rust decode/validation result:
official decode/validation result or log:
upstream source path and symbol:
reproduction command:
minimal failing input:
current hypothesis:
next verification step:
```

Do not patch around the failure by relaxing Rust validation or spoofing an official client string without understanding why the official node rejected the peer.

## M2 extension

After M1 passes, extend the same harness for:

```text
InvRequest / InvResponse
HeadersRequest / HeadersResponse
BlocksRequest / BlocksResponse
P2P V1/V2 differences
request timeout and correlation
real object decode/re-encode
```

M2 must retain all M1 assertions. Header/block exchange does not excuse handshake or liveness regressions.

### M2 assertions (current smoke-level bar)

- Rust sends `InvRequest` after successful Hello validation.
- Rust receives `ChainState` or logs fallback header-locator behavior (`sending fallback HeadersRequest with empty locator`) when chain-state is absent.
- Rust receives `InvResponse`, sends `HeadersRequest`, and observes at least one `HeadersResponse`.
- Rust sends `BlocksRequest` and observes at least one `BlocksResponse`.
- Rust sends `TxsRequest` and observes at least one `TxsResponse`.
- Optional stronger M2 checks:
  - set `RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS=1` or `strict` for strict local mode (requires at least one announcement marker);
  - set `RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS=all` for full-coverage mode (requires `NewBlock`, `NewHeader`, `NewInv`, and `NewTxHashes`);
  - set `RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS=warn` or `optional` to log and continue when announcements are not observed.
  - in `all` mode, the harness automatically enables `OFFICIAL_NODE_AUTO_MINE_FOR_DEV=1` and
    `OFFICIAL_NODE_ENABLE_WALLET=1` when starting a managed official node.
  - checks required in full-coverage mode:
    `NewBlock`, `NewHeader`, `NewInv`, and `NewTxHashes`.
- No request sequence should suppress Ping/Pong liveness behavior.

Run templates for M2:

- `make official-interop-m2` (default M2 without announcement assertions)
- `make official-interop-m2-optional-announcements` (announcement assertions optional)
- `make official-interop-m2-strict-announcements` (`1`/`strict`, at least one announcement marker required)
- `make official-interop-m2-all-announcements` (`all`, all announcement markers required)

CI automation note:

- `m1-official-interop` workflow also supports manual dispatch input `m2_announcements` with
  `none` (default), `strict`, `all`, or `warn` to choose announcement coverage.
