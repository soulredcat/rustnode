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

## End-to-end check against official releases

Run on 2026-10-15 on a local devnet with the official v4.7.2 release jar: 4 groups, genesis funds, the indexes in the retention table of `docs/NIXIA_API_SURFACE.md`, WebSocket on, API key on. Each request went to the official node directly and through the gateway, and status and JSON body were compared.

- 66 checks with real chain data, all identical: deploying a contract and a factory, sub-contract creation, `execute-script` build, `decode-unsigned-tx`, submit through the gateway, mempool contents before mining, status, blocks, blocks with events, rich details, events by tx id and by contract, event count, contract state, `call-contract` at the tip and pinned with `worldStateBlockHash`, `call-tx-script` with the bot's block-hash script, multicall, `transactions/build`, balance with `mempool=true`, UTXOs, error responses, missing and wrong API keys, and `/ws` acknowledgements, block and contract-event notifications, and unsubscribe. Same result with `address-group` in `shadow` and in `native` mode (with `RUSTNODE_API_KEY`).
- `nixiaproject`'s own `chain-alephium` crate, unmodified and built as a dependency of a probe: `NodeClient` (24 calls, including `snapshot_reader` pinned reads) and `ws::spawn` gave identical parsed results and frames (2 acknowledgements, 12 blocks, 3 contract events) through the gateway and directly.
- Official node restarted under a live subscription: the gateway answered `502` while it was down; the bot's client reconnected and resubscribed when it came back.
- 64 concurrent clients for 15 s: about 5,300 requests/s through the gateway with no errors, p99 27 ms (official node directly: 5,000 requests/s, p99 43 ms). Gateway resident memory 13 MiB; official node 575 MiB.
- With the v4.6.0 release, 1–7% of WebSocket connections never answered, directly and through the gateway alike: the official bug fixed in `5c40f665a`. With v4.7.2: 0 of 600.

The probe and the devnet harness (Node.js with `@alephium/web3`, a devnet wallet created at run time) are not kept in this repository.

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
