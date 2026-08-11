#![forbid(unsafe_code)]

fn main() {
    println!("alephium-node-rs");
    println!(
        "upstream baseline: {}",
        alephium_util::UPSTREAM_BASELINE_COMMIT
    );
    println!("configuration: {}", alephium_conf::STATUS);
    println!("flow: {}", alephium_flow::STATUS);
    println!("status: M0 workspace bootstrap only; P2P is not implemented");
}
