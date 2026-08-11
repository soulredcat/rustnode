#![forbid(unsafe_code)]

use std::process;
use std::thread;
use std::time::Duration;

use alephium_conf::{
    resolve_node_identity_path, NodeConfig, DEFAULT_PEER_ADDRESS,
};
use alephium_flow::run;

fn print_usage() {
    println!("Alephium Rust Node");
    println!("Usage:");
    println!("  alephium-node-rs [peer-address]");
    println!("  alephium-node-rs --peer <peer-address>");
    println!("  alephium-node-rs --watch");
    println!("  alephium-node-rs --help");
    println!();
    println!("Options:");
    println!("  --peer <addr>             override peer address (default: {DEFAULT_PEER_ADDRESS})");
    println!("  --watch                   keep retrying and reconnect when peer is unavailable");
    println!("  --watch-interval <secs>   delay between retries in watch mode (default: 5)");
    println!("  --watch-attempts <n>      max watch attempts before exiting (default: unlimited)");
}

fn run_with_retry(
    config: &NodeConfig,
    retry_interval: Duration,
    max_attempts: Option<usize>,
) -> ! {
    let mut attempt = 0usize;

    loop {
        attempt += 1;
        println!("starting rust watch session #{attempt} to {}", config.peer_address);

        if let Err(error) = run(config) {
            eprintln!("flow exited with error: {error}");
        }

        if let Some(limit) = max_attempts && attempt >= limit {
            eprintln!("watch mode reached attempt limit ({limit}); exiting");
            process::exit(1);
        }

        eprintln!(
            "watch mode retrying in {}s (attempt {attempt})",
            retry_interval.as_secs()
        );
        thread::sleep(retry_interval);
    }
}

fn main() {
    let mut peer_address = DEFAULT_PEER_ADDRESS.to_owned();
    let mut watch_mode = false;
    let mut watch_interval = Duration::from_secs(5);
    let mut watch_attempts: Option<usize> = None;

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_usage();
                return;
            }
            "--peer" => {
                let Some(next) = args.next() else {
                    eprintln!("--peer requires an argument: <host:port>");
                    process::exit(2);
                };
                peer_address = next;
            }
            arg if arg.starts_with("--peer=") => {
                peer_address = arg["--peer=".len()..].to_owned();
            }
            "--watch" => {
                watch_mode = true;
            }
            "--watch-interval" => {
                let Some(next) = args.next() else {
                    eprintln!("--watch-interval requires an argument: <seconds>");
                    process::exit(2);
                };
                let parsed = match next.parse::<u64>() {
                    Ok(value) => value,
                    Err(error) => {
                        eprintln!("invalid --watch-interval value '{next}': {error}");
                        process::exit(2);
                    }
                };
                watch_interval = Duration::from_secs(parsed);
            }
            arg if arg.starts_with("--watch-interval=") => {
                let value = &arg["--watch-interval=".len()..];
                match value.parse::<u64>() {
                    Ok(parsed) => watch_interval = Duration::from_secs(parsed),
                    Err(error) => {
                        eprintln!("invalid --watch-interval value '{value}': {error}");
                        process::exit(2);
                    }
                }
            }
            "--watch-attempts" => {
                let Some(next) = args.next() else {
                    eprintln!("--watch-attempts requires an argument: <attempt count>");
                    process::exit(2);
                };
                match next.parse::<usize>() {
                    Ok(value) => watch_attempts = Some(value),
                    Err(error) => {
                        eprintln!("invalid --watch-attempts value '{next}': {error}");
                        process::exit(2);
                    }
                }
            }
            arg if arg.starts_with("--watch-attempts=") => {
                let value = &arg["--watch-attempts=".len()..];
                match value.parse::<usize>() {
                    Ok(value) => watch_attempts = Some(value),
                    Err(error) => {
                        eprintln!("invalid --watch-attempts value '{value}': {error}");
                        process::exit(2);
                    }
                }
            }
            arg if arg.starts_with("-") => {
                eprintln!("unknown option: {arg}");
                eprintln!("run --help for usage");
                process::exit(2);
            }
            arg => {
                peer_address = arg.to_owned();
            }
        }
    }

    let identity_path = resolve_node_identity_path();
    let config = match NodeConfig::with_peer_address(peer_address).with_node_identity_path(identity_path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load node identity: {error}");
            process::exit(1);
        }
    };

    println!("alephium-node-rs");
    println!("upstream baseline: {}", alephium_util::UPSTREAM_BASELINE_COMMIT);
    println!("configuration: {}", alephium_conf::STATUS);
    println!("node config: peer={}, client={}", config.peer_address, config.client_id);
    println!("flow: {}", alephium_flow::STATUS);

    if watch_mode {
        run_with_retry(&config, watch_interval, watch_attempts);
    } else if let Err(error) = run(&config) {
        eprintln!("flow exited with error: {error}");
        process::exit(1);
    }
}
