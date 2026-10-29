# Mainnet check

rustnode is checked against the operator's synced official mainnet node, the only network it serves (`IMMUTABLE.md`). Rust-only round trips are not evidence; the official node and the official Scala code are the oracles.

## Run

```bash
make mainnet-check
```

It needs the official mainnet node with its REST API on `127.0.0.1:12973` and P2P on `127.0.0.1:9973` (override with `OFFICIAL_URL`, `OFFICIAL_PEER`, `API_BIND`, `WINDOW`). It takes about five minutes and puts a bootstrap load on the official node: `WINDOW` heights of 16 chains (300 by default), fetched over REST with 8 threads.

`scripts/mainnet-check.sh` asserts, in order:

1. the official node serves network id 0 and is synced;
2. the newest 100 heights of every chain round-trip byte for byte and render the official block JSON (`official_raw_blocks`, `official_block_json`), and the newest 30 heights pass header validation, work target included, with settled heights on the official canonical chain (`official_headers`);
3. a fresh rustnode bootstraps `WINDOW` heights per chain into its store, completes the official `Hello` and a Ping/Pong round trip, seeds weights from the peer's `ChainState`, follows new blocks over P2P v2, each passing header validation, with weights equal to the peer's, serves intra-group blocks across the window and the status of their transactions (with and without groups) in `shadow` mode with `shadow_mismatch=0`; polls the mempool for `MEMPOOL_SECONDS` (default 180) in `shadow` mode with no mismatch, which allows only transactions the official node saw less than 15 s ago to be missing (it relays transactions in batches), asks the status of transactions held there for over 30 s (`MemPooled`) and of an unknown transaction (`TxNotFound`), and requires that rustnode held mempool transactions itself; and its gateway answers `GET /infos/node` with official build info, refuses `GET /infos/self-clique` (outside the surface) with `404`, and returns exactly the official status and body for `GET /addresses/{address}/group` (official `AddressSpec` vectors, outputs of the newest mainnet blocks, a contract, a grouped and an invalid address);
4. killed and restarted 30 s later, rustnode reopens its store, fetches only the heights it missed, and keeps following with equal weights.

The official `Hello` is verified by the official node itself: it drops a peer whose `Hello` or broker signature does not check, so a session that reaches Ping/Pong and block download proves the handshake.

## Evidence

2026-10-27, official v4.7.2 mainnet node on the operator's machine, `--window 300`, by hand:

- bootstrap of 4,816 heights (5,193 blocks) into the store in about 6 s;
- 246 blocks followed over P2P, 0 orphans, 9 weight checks against the peer, 0 mismatches;
- `block-by-hash` shadow: 16 of 16 blocks equal, from the tip to 295 heights back;
- killed with SIGTERM, restarted 66 s later: 144 missed heights (162 blocks) fetched instead of 4,816, one `ChainState` seeded their weights, then 283 blocks followed with 61 weight checks and 0 mismatches; shadow 24 of 24 equal across blocks from before the kill, from the catch-up, and newly followed;
- resident memory 28–32 MiB; store file 16.8 MB, unchanged while hundreds more blocks arrived, since heights leaving the window are deleted.

2026-10-29, `make mainnet-check` after the mainnet-only cleanup: the newest 100 heights of all 16 chains round-trip and render the official JSON; bootstrap of 4,816 heights (5,198 blocks); 13 address groups and 16 of 16 shadow-checked blocks equal to the official node; 114 blocks followed, 13 weight checks, 0 mismatches; resident memory 33 MiB, store 8.4 MB on disk; after the kill, 75 missed heights fetched instead of 4,800, then 23 blocks followed with 7 weight checks and 0 mismatches.

2026-10-29, `make mainnet-check` with the native mempool: bootstrap of 4,816 heights (5,154 blocks); 16 of 16 blocks, 54 of 54 transaction statuses (36 confirmed, 15 `MemPooled`, 3 `TxNotFound`) and 84 of 84 mempools (116 transactions listed) equal to the official node; 100 blocks followed, 7 weight checks, 0 mismatches; resident memory 27 MiB, store 9.4 MB; after the kill, 56 missed heights fetched instead of 4,800. An earlier run that asked statuses before the peer's `ChainState` had given every chain a weight answered 0 confirmations for two chains (18 mismatches); statuses are now declined until every searched chain has a weight, and the check waits for `unweighted chains=0`.

`address-group` runs `native` by default (`native::verified`) because step 3 compares it with the official node on every run.

## Failure report

When the check fails, record:

```text
first failing step:
rustnode log lines around it:
official node log or response:
upstream source path and symbol:
reproduction command:
current hypothesis:
```

Do not patch around a failure by relaxing a check or spoofing an official answer without understanding why the official node disagrees.
