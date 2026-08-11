#![forbid(unsafe_code)]

use std::process;

use alephium_conf::{
    resolve_node_identity_path, NodeConfig, DEFAULT_PEER_ADDRESS,
};
use alephium_flow::run;

fn main() {
    let mut peer_address = DEFAULT_PEER_ADDRESS.to_owned();

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("Alephium Rust Node");
                println!("Usage:");
                println!("  alephium-node-rs [peer-address]");
                println!("  alephium-node-rs --peer <peer-address>");
                println!("  alephium-node-rs --help");
                println!();
                println!("Options:");
                println!("  --peer <addr>   override peer address (default: {DEFAULT_PEER_ADDRESS})");
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
    println!(
        "node config: peer={}, client={}",
        config.peer_address, config.client_id
    );
    println!("flow: {}", alephium_flow::STATUS);

    if let Err(error) = run(&config) {
        eprintln!("flow exited with error: {error}");
        process::exit(1);
    }
}
