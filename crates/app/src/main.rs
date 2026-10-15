#![forbid(unsafe_code)]

use std::process;
use std::thread;
use std::time::Duration;

use alephium_api::Gateway;
use alephium_conf::{
    API_KEY_ENV, ApiConfig, DEFAULT_API_BIND_ADDRESS, DEFAULT_UPSTREAM_URL, NodeConfig,
    resolve_node_identity_path,
};
use alephium_flow::{RawMempool, SharedChainStore, SharedRawMempool, run};

fn print_usage() {
    println!("rustnode: Alephium node for nixiaproject");
    println!("Usage:");
    println!("  rustnode [options] [peer-address]");
    println!();
    println!("API gateway (always on; point nixiaproject's ALEPHIUM_NODE_URL here):");
    println!(
        "  --api-bind <addr>             listen address (default: {DEFAULT_API_BIND_ADDRESS})"
    );
    println!("  --upstream <url>              official node (default: {DEFAULT_UPSTREAM_URL})");
    println!("  --route-mode <id>=<mode>      proxy, shadow, or native; repeatable");
    println!("                                (verified native handlers default to native,");
    println!("                                new ones to shadow)");
    println!("  env {API_KEY_ENV}          the official node's api-key, if it has one; natively");
    println!("                                answered requests must carry it");
    println!();
    println!("P2P transaction acquisition (only when a peer is given):");
    println!("  --peer <addr>                 Alephium P2P peer, same as the positional argument");
    println!("  --network-id <id>             0 mainnet (default), 1 testnet, other ids devnet");
    println!("  --window <heights>            heights kept per chain (default: 10000)");
    println!("  --max-inflight <n>            concurrent TxsRequest limit");
    println!("  --tx-batch-size <n>           must be 1 until typed tx parsing is complete");
    println!("  --request-timeout-ms <ms>     retry timeout for TxsRequest");
    println!("  --watch                       reconnect after peer failure");
    println!("  --watch-interval <seconds>    reconnect delay (default: 2)");
    println!();
    println!("  --help                        show this help");
}

fn required_value(flag: &str, value: Option<String>) -> String {
    value.unwrap_or_else(|| {
        eprintln!("{flag} requires a value");
        process::exit(2);
    })
}

fn parse_usize(flag: &str, value: Option<String>) -> usize {
    let value = required_value(flag, value);
    value.parse::<usize>().unwrap_or_else(|error| {
        eprintln!("invalid {flag} value '{value}': {error}");
        process::exit(2);
    })
}

fn parse_u64(flag: &str, value: Option<String>) -> u64 {
    let value = required_value(flag, value);
    value.parse::<u64>().unwrap_or_else(|error| {
        eprintln!("invalid {flag} value '{value}': {error}");
        process::exit(2);
    })
}

fn run_with_reconnect(
    config: &NodeConfig,
    mempool: &SharedRawMempool,
    chain: &SharedChainStore,
    interval: Duration,
) -> ! {
    let mut attempt = 0u64;
    loop {
        attempt = attempt.saturating_add(1);
        eprintln!(
            "starting P2P peer attempt {attempt} against {}",
            config.peer_address
        );
        if let Err(error) = run(config, mempool, Some(chain)) {
            eprintln!("P2P session stopped: {error}");
        }
        thread::sleep(interval);
    }
}

fn main() {
    let mut config = NodeConfig::default().set_network_id(0);
    let mut api = ApiConfig::default();
    let mut peer_given = false;
    let mut watch = false;
    let mut watch_interval = Duration::from_secs(2);
    let mut window = alephium_chain::DEFAULT_WINDOW;
    let mut args = std::env::args().skip(1);

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                print_usage();
                return;
            }
            "--api-bind" => api.bind_address = required_value("--api-bind", args.next()),
            "--upstream" => api.upstream_url = required_value("--upstream", args.next()),
            "--route-mode" => api
                .route_modes
                .push(required_value("--route-mode", args.next())),
            "--peer" => {
                config = config.set_peer_address(required_value("--peer", args.next()));
                peer_given = true;
            }
            "--network-id" => {
                let value = parse_u64("--network-id", args.next());
                let Ok(network_id) = u8::try_from(value) else {
                    eprintln!("--network-id must be at most 255");
                    process::exit(2);
                };
                config = config.set_network_id(network_id);
            }
            "--window" => {
                let value = parse_u64("--window", args.next());
                window = u32::try_from(value).unwrap_or(u32::MAX).max(128);
            }
            "--max-inflight" => {
                let value = parse_usize("--max-inflight", args.next());
                config = config.set_max_inflight_tx_requests(value);
            }
            "--tx-batch-size" => {
                let value = parse_usize("--tx-batch-size", args.next());
                if value != 1 {
                    eprintln!("--tx-batch-size must be 1 for canonical raw decoding");
                    process::exit(2);
                }
                config = config.set_tx_request_batch_size(value);
            }
            "--request-timeout-ms" => {
                let value = parse_u64("--request-timeout-ms", args.next());
                config = config.set_tx_request_timeout(Duration::from_millis(value));
            }
            "--watch" => watch = true,
            "--watch-interval" => {
                watch_interval = Duration::from_secs(parse_u64("--watch-interval", args.next()));
            }
            value if value.starts_with('-') => {
                eprintln!("unknown option: {value}");
                process::exit(2);
            }
            peer_address => {
                config = config.set_peer_address(peer_address.to_owned());
                peer_given = true;
            }
        }
    }

    api.api_key = std::env::var(API_KEY_ENV)
        .ok()
        .filter(|key| !key.is_empty());
    api.network_id = config.network_id;
    // Followed blocks, written by the P2P session and read by native handlers.
    let chain: SharedChainStore = std::sync::Arc::default();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| {
            eprintln!("failed to start async runtime: {error}");
            process::exit(1);
        });
    let mut gateway = runtime
        .block_on(Gateway::bind(api.clone()))
        .unwrap_or_else(|error| {
            eprintln!("{error}");
            process::exit(1);
        });
    if peer_given {
        gateway = gateway.with_chain(chain.clone());
    }

    println!("rustnode");
    println!("configuration: {}", alephium_conf::STATUS);
    match gateway.local_addr() {
        Ok(address) => println!("API: http://{address} -> {}", gateway.upstream_base()),
        Err(_) => println!("API: {} -> {}", api.bind_address, gateway.upstream_base()),
    }
    for (route, mode) in gateway.non_proxy_routes() {
        println!("route {} ({}): {}", route.id(), route.name(), mode.name());
    }
    if api.api_key.is_some() {
        println!("api key: required on native answers (from {API_KEY_ENV})");
    }

    if peer_given {
        config = config
            .with_node_identity_path(resolve_node_identity_path())
            .unwrap_or_else(|error| {
                eprintln!("failed to load P2P identity: {error}");
                process::exit(1);
            });
        println!("peer: {}", config.peer_address);
        println!("runtime: {}", alephium_flow::STATUS);
        let mempool = RawMempool::shared(config.max_mempool_txs);
        let bootstrap_source =
            alephium_flow::bootstrap::Source::new(&api.upstream_url, api.api_key.clone());
        thread::spawn(move || {
            // One-time start from the official node's recent blocks; without
            // it, blocks are followed from genesis over P2P.
            match alephium_flow::bootstrap::bootstrap(
                &bootstrap_source,
                config.configured_groups,
                config.network_id,
                window,
                8,
            ) {
                Ok(store) => {
                    *chain
                        .write()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(store);
                }
                Err(error) => eprintln!("bootstrap from the official node failed: {error}"),
            }
            if watch {
                run_with_reconnect(&config, &mempool, &chain, watch_interval);
            }
            if let Err(error) = run(&config, &mempool, Some(&chain)) {
                eprintln!("P2P session stopped: {error}");
                process::exit(1);
            }
        });
    } else {
        println!("peer: none (P2P acquisition off)");
    }

    runtime.block_on(gateway.run());
}
