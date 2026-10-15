# nixiaproject API Surface

This is the complete list of what `soulredcat/nixiaproject` reads from an Alephium node. It is the specification for `rustnode`: serve exactly these, with the same paths, parameters, and JSON shapes as the official node, and nothing else.

`nixiaproject` is read-only for this repository. It reaches the node through one setting, `ALEPHIUM_NODE_URL` (default `http://127.0.0.1:12973`), plus an optional `ALEPHIUM_NODE_API_KEY`. The WebSocket URL is derived from the same base with path `/ws`. Switching the bot to `rustnode` must require only changing `ALEPHIUM_NODE_URL`.

Source of this inventory: `chain/alephium/src/node.rs` (`NodeClient`), `chain/alephium/src/ws/`, and their callers, at `nixiaproject` commit `5558a46`.

## HTTP

Legend for **needs**: `B` recent blocks, `M` mempool, `S` world state, `VM` Ralph VM execution, `E` event log index, `I` other node index, `P2P` broadcast.

| Endpoint | Needs | Main callers in nixiaproject |
| --- | --- | --- |
| `GET /mempool/transactions` | M | `runner/src/alephium_mempool.rs` (hot path, polled every 25 ms by default) |
| `GET /contracts/{address}/state` | S | event refresh, live quote snapshot, every DEX adapter, token discovery |
| `POST /contracts/call-contract` | S at a recent block, VM | `runner/src/alephium_prebuild.rs`, `dex/elexium/src/fee.rs` |
| `POST /contracts/call-tx-script` | S at a recent block, VM | live quote snapshot, `chain/alephium/src/node_snapshot.rs` |
| `POST /contracts/multicall-contract` | S, VM | `chain/alephium/src/token_metadata.rs` |
| `POST /contracts/unsigned-tx/execute-script` | S, VM, UTXO selection, gas estimation | `chain/alephium/src/execute_script.rs` (prebuild) |
| `POST /transactions/decode-unsigned-tx` | S (input lookup) | `runner/src/alephium_prebuild.rs` |
| `POST /transactions/build` | S, UTXO selection | `background/wallet-rebalance` |
| `POST /transactions/submit` | S (validation), M, P2P | `chain/alephium/src/submit.rs` |
| `GET /transactions/status?txId=` | B, M | tx confirmation, block-tx audit, ranking, wallet rebalance |
| `GET /transactions/rich-details/{txId}` | B, I (`tx-output-ref-index`) | `runner/src/tx_confirmation.rs` |
| `GET /addresses/{address}/balance?mempool=true` | S, M | wallet book, prebuild, runner startup, wallet rebalance |
| `GET /addresses/{address}/utxos` | S | wallet book, prebuild, wallet rebalance |
| `GET /addresses/{address}/group` | — (pure function of the address) | runner startup |
| `GET /blockflow/blocks/{hash}` | B | `runner/src/alephium_block_tx_audit.rs` |
| `GET /blockflow/blocks?fromTs=&toTs=` | B | event refresh, block-tx audit |
| `GET /blockflow/blocks-with-events/{hash}` | B, E (`index-by-block-hash`) | `runner/src/alephium_event_refresh.rs` (main event path) |
| `GET /events/contract/{address}?start=&limit=` | E | `ws/backfill.rs`, token discovery |
| `GET /events/contract/{address}/current-count` | E | `ws/backfill.rs` |
| `GET /events/tx-id/{txId}` | E (`index-by-tx-id`) | `runner/src/alephium_ranking.rs` |
| `GET /contracts/{address}/sub-contracts?start=&limit=` | I (`subcontract-index`) | powfi CLMM discovery, token discovery |
| `GET /infos/node` | — (build info) | `tools/check-system-health`, `tools/historical-transaction-accounting` |

## WebSocket `/ws` (JSON-RPC 2.0)

| Message | Needs |
| --- | --- |
| `subscribe ["block"]` → `subscription` notifications of type `Block` | B |
| `subscribe ["contract", {"addresses": [...]}]` → notifications of type `ContractEvent` | E, VM |
| `unsubscribe ["<subscriptionId>"]` | — |
| `server_shutdown` notification | — |

`nixiaproject` does not subscribe to mempool transaction notifications.

`GET /infos/node` is only called by `nixiaproject` tools, which read the same `ALEPHIUM_NODE_URL`; it is served so they keep working after the switch. The other tools (`transfer-pin-check`, `clamm-route-simulate`, `historical-transaction-accounting`) call only endpoints already in the table.

## Read pinning

`chain/alephium/src/node_snapshot.rs` asks the VM for the current block hash of each group, then sends `call-tx-script` and `call-contract` requests with `worldStateBlockHash` set to that block, so related contracts are read from one immutable state. A native implementation must answer reads at any block inside the reorg state window, not only at the tip.

## Retention needed by the bot

| Data | Keep | Why |
| --- | --- | --- |
| World state | current, plus the last 128 heights per chain | reorgs and pinned reads (same window as the official `PruneStorageService`) |
| Blocks and their events | the last 10,000 heights per chain: 160,000 blocks over 16 chains, about 22 hours at the 8 s Danube block time | event-refresh gap recovery, block-tx audit, status and events of the bot's own recent transactions |
| Spent outputs of transactions in retained blocks | same window as blocks | `rich-details` resolves every input, and an input can be much older than the window |
| Events of the DEX factory contracts the bot scans | permanently | `background/token-discovery` reads factory events from index 0 |
| Sub-contract index | permanently | factory sub-contracts are listed from index 0 |
| Mempool | official limits (2000 per chain) | `GET /mempool/transactions` |

If the bot is stopped for longer than the block window (about 22 hours), gap recovery cannot reread the gap and the bot must reload state, as it already does after a long stop. The window is configurable; it must not go below the 128 heights of the state window, and raising it only costs disk, since blocks are looked up by hash or height.

## Not served

- `WALLET_REBALANCE_EXPLORER_API_URL` points `chain/alephium/src/wallet_activity.rs` at an explorer backend, not at the node. Out of scope.
- Every other official endpoint (wallet, miners, multisig, utils, other `infos`, ...) answers `404` from `rustnode`.

## Served by rustnode today

Every endpoint above, and `/ws`, is accepted by the gateway and proxied to the official node (N0 in `TODO.md`), except:

| Route id | Native handler | Default mode |
| --- | --- | --- |
| `address-group` | asset addresses (P2PKH, P2MPKH, P2SH); contract, grouped, and invalid addresses are proxied | `native` (verified in `docs/INTEROPERABILITY.md`) |

A native answer is given only when the request passes the official API-key check: its `X-API-KEY` equals `RUSTNODE_API_KEY`, or both are absent. Any other request is proxied and gets the official error.

Route ids for `--route-mode`: `mempool-transactions`, `contract-state`, `call-contract`, `call-tx-script`, `multicall-contract`, `execute-script`, `decode-unsigned-tx`, `build-transaction`, `submit-transaction`, `transaction-status`, `transaction-rich-details`, `address-balance`, `address-utxos`, `address-group`, `block-by-hash`, `blocks-by-time`, `block-with-events`, `contract-events`, `contract-event-count`, `events-by-tx-id`, `sub-contracts`, `infos-node`, `ws`.

## What this implies

Only `GET /addresses/{address}/group` and `GET /infos/node` are free. Everything else needs blocks, mempool, world state, the VM, or indexes, so the node must follow the chain and execute every block. See `TODO.md` for the staged plan, which serves each endpoint natively only after it matches the official node on live traffic.
