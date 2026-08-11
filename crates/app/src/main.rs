#![forbid(unsafe_code)]

use std::process;

use alephium_conf::{NodeConfig, DEFAULT_PEER_ADDRESS};
use alephium_flow::run;

fn main() {
    let mut peer_address = DEFAULT_PEER_ADDRESS.to_owned();
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        peer_address = args[1].clone();
    }

    let config = NodeConfig::with_peer_address(peer_address);

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
