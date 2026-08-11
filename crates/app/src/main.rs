#![forbid(unsafe_code)]

use std::process;

use alephium_conf::{
    resolve_node_identity_path, NodeConfig, DEFAULT_PEER_ADDRESS,
};
use alephium_flow::run;

fn main() {
    let mut peer_address = DEFAULT_PEER_ADDRESS.to_owned();
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        peer_address = args[1].clone();
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
