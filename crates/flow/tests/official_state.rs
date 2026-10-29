//! The output state transition against the official node: for recent
//! intra-group blocks, the world state of the block's parent (read from the
//! official node's database) with the block's view and the block applied
//! must have the official output root of the block.
//!
//! ```text
//! RUSTNODE_OFFICIAL_DB=<official db folder> \
//!   cargo test --release -p alephium-flow --test official_state -- --ignored --nocapture
//! ```

use alephium_chain::Disk;
use alephium_flow::bootstrap::{Source, start};
use alephium_flow::execute::apply_outputs;
use alephium_state::{Official, WorldState};

const GROUPS: u32 = 4;
const WINDOW: u32 = 128;
/// Blocks checked per group, below the newest settled heights.
const BLOCKS: u32 = 30;
const SETTLED: u32 = 10;

#[test]
#[ignore = "needs the operator's official node database and REST API"]
fn outputs_follow_the_official_state() {
    let url = std::env::var("RUSTNODE_OFFICIAL_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:12973".to_owned());
    let db = std::env::var("RUSTNODE_OFFICIAL_DB").expect("RUSTNODE_OFFICIAL_DB");
    let work = std::env::temp_dir().join(format!("rustnode-official-state-{}", std::process::id()));
    std::fs::create_dir_all(&work).expect("work dir");
    let disk = Disk::open(&work.join("chain.redb")).expect("disk");
    let chain = start(&Source::new(&url, None), GROUPS, WINDOW, 8, disk).expect("bootstrap");
    let official = Official::open(db.as_ref(), &work.join("secondary")).expect("official db");

    let (mut matched, mut checked) = (0, 0);
    for group in 0..GROUPS {
        let top = chain.max_height(group, group) - SETTLED;
        for height in top - BLOCKS..top {
            let hash = chain.hashes_at(group, group, height)[0];
            let block = std::sync::Arc::clone(&chain.get(&hash).expect("block").block);
            let parent = block.header.deps[(GROUPS - 1 + group) as usize];
            let before = official
                .roots(&parent)
                .expect("read")
                .expect("parent roots");
            let after = official.roots(&hash).expect("read").expect("block roots");
            let mut world = WorldState::new(&before, &official);
            let conflicts =
                apply_outputs(&chain, &mut world, &block, GROUPS, |_, _| false).expect("apply");
            checked += 1;
            if world.roots().outputs == after.outputs {
                matched += 1;
            } else {
                println!(
                    "group {group} height {height} block {}: outputs root {} instead of {} ({} txs, conflicts {conflicts:?})",
                    hash.to_hex(),
                    world.roots().outputs.to_hex(),
                    after.outputs.to_hex(),
                    block.transactions.len()
                );
            }
        }
    }
    println!("{matched} of {checked} blocks reproduce the official output root");
    drop(official);
    let _ = std::fs::remove_dir_all(&work);
    assert_eq!(matched, checked);
}
