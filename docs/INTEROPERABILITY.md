# Official-Node Interoperability

## Goal

Prove that the rustnode speaks the official Alephium protocol directly and is accepted by an unmodified official Scala node.

Interoperability is not inferred from similar structures or Rust-only round trips. It requires evidence from both implementations: Scala-generated vectors (`tests/vectors/m1/`) and a live run against the pinned official node (`UPSTREAM.toml`).

## Non-negotiable test rules

The topology must be:

```text
Official Scala node <---- raw Alephium TCP/P2P ----> rustnode
```

Forbidden in compatibility tests:

- protocol-rewriting proxies or JSON-to-P2P gateways;
- modified official-node validation or skipped signature checks;
- hardcoded acceptance of the Rust peer;
- replaying a captured official packet as if Rust produced it;
- declaring success after TCP connect without a valid Hello exchange.

A packet capture may observe traffic, but it must not modify it.

## Environments, in order

1. Rust unit tests and Scala-generated vectors inside the workspace.
2. Local isolated official node (`docs/LOCAL_OFFICIAL_NODE.md`).
3. Official testnet peer, after local tests pass.
4. Mainnet peer, only after safety review. No mining, signing, or funds are involved.

Do not point malformed-frame experiments at public peers.

## Harness

```bash
make official-interop-fastpath
```

The harness builds the pinned official node, starts it on an isolated devnet, runs the rustnode against it, and asserts:

- `received valid Hello` — the official Hello decoded and its broker signature verified;
- `received matching pong` — a Ping/Pong round trip completed;
- the API gateway, pointed at the same official node, returns official build info for `GET /infos/node`, returns the same body as the official node for `GET /mempool/transactions`, and answers `404` for `GET /infos/self-clique`, which is outside the surface;
- with `address-group` in `native` mode, `GET /addresses/{address}/group` returns exactly the official status and body for P2PKH, P2MPKH, and P2SH vectors, devnet genesis addresses, a contract address, a grouped address, and an invalid one.

Artifacts, the run manifest, and environment variables are described in `tests/interoperability/fastpath/README.md`. CI runs the same target in `.github/workflows/official-fastpath-interop.yml`.

Not asserted yet: transaction-announcement interop (`NewTxHashes` → `TxsRequest` → `RawTransaction`). See `TODO.md`.

## Failure report

When interoperability fails, record:

```text
first divergent stage:
last accepted frame:
raw frame hash/fixture reference:
Rust decode/validation result:
official decode/validation result or log:
upstream source path and symbol:
reproduction command:
current hypothesis:
```

Do not patch around a failure by relaxing Rust validation or spoofing an official client string without understanding why the official node rejected the peer.
