# IMMUTABLE

These rules do not change. A change that breaks one is rejected, whoever proposes it. Only the repository owner changes this file.

## 1. Mainnet only

rustnode serves Alephium mainnet and nothing else: network id 0, 4 groups, frame magic `b0a6808e`, the official mainnet bootstrap peers, and the mainnet hard-fork times and mining floor (`crates/model/src/network.rs`, `crates/conf/src/lib.rs`).

Devnet and testnet are forbidden. No code, configuration, flag, constant, branch, test, fixture, script, CI job, or document may target, select, or describe them. Anything found is deleted at once, not disabled, gated, or kept for later. There is no network switch: no `--network-id`, no network parameter, no per-network table.

## 2. Only what is built and used

Every crate, module, function, type, constant, dependency, flag, file, and CI job must be used by the `rustnode` binary, or by a test or check of what the binary uses. Library code that only its own tests call is dead.

Dead code is deleted immediately. It is never commented out, feature-gated, or kept for a later stage; git history keeps it.

## 3. Clean build

Every commit passes `make check` (rustfmt, `cargo check`, `clippy -D warnings`, and every test) with no warning.

## 4. Verified against mainnet

Compatibility evidence comes from the pinned official implementation (`UPSTREAM.toml`) generating mainnet vectors (`make m1-vectors`), and from the operator's synced official mainnet node (`make mainnet-check`). Nothing else counts as evidence.

## 5. Tests take at most one minute

Every test and check finishes within 60 seconds: `make check` as a whole, `make mainnet-check` as a whole, and any single test run by hand. A check that needs longer is shrunk, not kept. Few small checks beat many large ones; the node's own `depStateHash` check of every followed block is the standing proof against the network.

## 6. Under 200 GB on disk

All of rustnode's data together (block window, world state with its history, events and indexes, logs) stays under 200 GB on mainnet. Going over is a failure, not a tuning matter: what grows without bound is pruned to the block window.
