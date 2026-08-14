#![forbid(unsafe_code)]

use std::process;
use std::thread;
use std::time::Duration;

use alephium_conf::{
    DEFAULT_IPC_BIND_ADDRESS, DEFAULT_PEER_ADDRESS, NodeConfig, resolve_node_identity_path,
};
use alephium_flow::{EventSink, IpcPublisher, run};

fn print_usage() {
    println!("Alephium Bot Fast-Path");
    println!("Usage:");
    println!("  alephium-bot-fastpath [peer-address]");
    println!("  alephium-bot-fastpath --peer <host:port> --ipc-bind <host:port>");
    println!();
    println!("Options:");
    println!("  --peer <addr>                 Alephium P2P peer (default: {DEFAULT_PEER_ADDRESS})");
    println!(
        "  --ipc-bind <addr>             loopback bot event server (default: {DEFAULT_IPC_BIND_ADDRESS})"
    );
    println!("  --max-inflight <n>            concurrent TxsRequest limit");
    println!("  --tx-batch-size <n>           maximum transaction IDs per request");
    println!("  --request-timeout-ms <ms>     retry timeout for TxsRequest");
    println!("  --event-queue-capacity <n>    bounded IPC event queue");
    println!("  --watch                       reconnect after peer failure");
    println!("  --watch-interval <seconds>    reconnect delay (default: 2)");
    println!("  --help                        show this help");
}

fn parse_usize(flag: &str, value: Option<String>) -> usize {
    let Some(value) = value else {
        eprintln!("{flag} requires a value");
        process::exit(2);
    };
    value.parse::<usize>().unwrap_or_else(|error| {
        eprintln!("invalid {flag} value '{value}': {error}");
        process::exit(2);
    })
}

fn parse_u64(flag: &str, value: Option<String>) -> u64 {
    let Some(value) = value else {
        eprintln!("{flag} requires a value");
        process::exit(2);
    };
    value.parse::<u64>().unwrap_or_else(|error| {
        eprintln!("invalid {flag} value '{value}': {error}");
        process::exit(2);
    })
}

fn run_with_reconnect(config: &NodeConfig, events: &EventSink, interval: Duration) -> ! {
    let mut attempt = 0u64;
    loop {
        attempt = attempt.saturating_add(1);
        eprintln!(
            "starting fast-path peer attempt {attempt} against {}",
            config.peer_address
        );
        if let Err(error) = run(config, events) {
            eprintln!("fast-path session stopped: {error}");
        }
        thread::sleep(interval);
    }
}

fn main() {
    let mut config = NodeConfig::default();
    let mut watch = false;
    let mut watch_interval = Duration::from_secs(2);
    let mut args = std::env::args().skip(1).peekable();

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                print_usage();
                return;
            }
            "--peer" => {
                let Some(value) = args.next() else {
                    eprintln!("--peer requires <host:port>");
                    process::exit(2);
                };
                config = config.set_peer_address(value);
            }
            "--ipc-bind" => {
                let Some(value) = args.next() else {
                    eprintln!("--ipc-bind requires <host:port>");
                    process::exit(2);
                };
                config = config.set_ipc_bind_address(value);
            }
            "--max-inflight" => {
                let value = parse_usize("--max-inflight", args.next());
                config = config.set_max_inflight_tx_requests(value);
            }
            "--tx-batch-size" => {
                let value = parse_usize("--tx-batch-size", args.next());
                config = config.set_tx_request_batch_size(value);
            }
            "--request-timeout-ms" => {
                let value = parse_u64("--request-timeout-ms", args.next());
                config = config.set_tx_request_timeout(Duration::from_millis(value));
            }
            "--event-queue-capacity" => {
                let value = parse_usize("--event-queue-capacity", args.next());
                config = config.set_event_queue_capacity(value);
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
            }
        }
    }

    config = config
        .with_node_identity_path(resolve_node_identity_path())
        .unwrap_or_else(|error| {
            eprintln!("failed to load P2P identity: {error}");
            process::exit(1);
        });

    let publisher = IpcPublisher::bind(&config.ipc_bind_address, config.event_queue_capacity)
        .unwrap_or_else(|error| {
            eprintln!(
                "failed to bind bot IPC {}: {error}",
                config.ipc_bind_address
            );
            process::exit(1);
        });
    let events = publisher.sink();

    println!("alephium-bot-fastpath");
    println!("peer: {}", config.peer_address);
    println!("bot IPC: {}", config.ipc_bind_address);
    println!("runtime: {}", alephium_flow::STATUS);
    println!("configuration: {}", alephium_conf::STATUS);

    if watch {
        run_with_reconnect(&config, &events, watch_interval);
    }

    if let Err(error) = run(&config, &events) {
        eprintln!("fast-path session stopped: {error}");
        process::exit(1);
    }
}
