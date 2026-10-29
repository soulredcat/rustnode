//! The state transition against the official node: for recent intra-group
//! blocks, the world state of the block's parent (read from the official
//! node's database) with the block's view and the block applied, scripts
//! replayed by the VM, must have the official output and contract roots of
//! the block.
//!
//! ```text
//! RUSTNODE_OFFICIAL_DB=<official db folder> \
//!   [RUSTNODE_STATE_BLOCKS=<blocks per group, default 30>] \
//!   cargo test --release -p alephium-flow --test official_state -- --ignored --nocapture
//! ```

use alephium_chain::Disk;
use alephium_flow::bootstrap::{Source, start};
use alephium_flow::execute::apply_block;
use alephium_state::{Official, WorldState};

const GROUPS: u32 = 4;
const SETTLED: u32 = 10;
/// Heights kept below the checked ones, for the blocks their views add.
const MARGIN: u32 = 100;

#[test]
#[ignore = "needs the operator's official node database and REST API"]
fn blocks_reproduce_the_official_state() {
    let url = std::env::var("RUSTNODE_OFFICIAL_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:12973".to_owned());
    let db = std::env::var("RUSTNODE_OFFICIAL_DB").expect("RUSTNODE_OFFICIAL_DB");
    let work = std::env::temp_dir().join(format!("rustnode-official-state-{}", std::process::id()));
    std::fs::create_dir_all(&work).expect("work dir");
    // Blocks checked per group, below the newest settled heights.
    let blocks: u32 = std::env::var("RUSTNODE_STATE_BLOCKS")
        .ok()
        .and_then(|blocks| blocks.parse().ok())
        .unwrap_or(30);
    let disk = Disk::open(&work.join("chain.redb")).expect("disk");
    let window = (blocks + SETTLED + MARGIN).max(128);
    let chain = start(&Source::new(&url, None), GROUPS, window, 8, disk).expect("bootstrap");
    let official = Official::open(db.as_ref(), &work.join("secondary")).expect("official db");

    let (mut matched, mut checked, mut scripts) = (0, 0, 0);
    for group in 0..GROUPS {
        let top = chain.max_height(group, group) - SETTLED;
        for height in top - blocks..top {
            let hash = chain.hashes_at(group, group, height)[0];
            let block = std::sync::Arc::clone(&chain.get(&hash).expect("block").block);
            let parent = block.header.deps[(GROUPS - 1 + group) as usize];
            let before = official
                .roots(&parent)
                .expect("read")
                .expect("parent roots");
            let after = official.roots(&hash).expect("read").expect("block roots");
            let mut world = WorldState::new(&before, &official);
            checked += 1;
            match apply_block(&chain, &mut world, &block, GROUPS, |_, _| false) {
                Ok(applied) => {
                    scripts += applied.executions.len();
                    let roots = world.roots();
                    if roots.outputs == after.outputs && roots.contracts == after.contracts {
                        matched += 1;
                    } else {
                        println!(
                            "group {group} height {height} block {}: outputs {} contracts {} ({} txs)",
                            hash.to_hex(),
                            if roots.outputs == after.outputs {
                                "equal"
                            } else {
                                "different"
                            },
                            if roots.contracts == after.contracts {
                                "equal"
                            } else {
                                "different"
                            },
                            block.transactions.len()
                        );
                    }
                }
                Err(error) => println!(
                    "group {group} height {height} block {}: {error}",
                    hash.to_hex()
                ),
            }
        }
    }
    println!("{scripts} transaction scripts replayed");
    println!("{matched} of {checked} blocks reproduce the official output and contract roots");
    drop(official);
    let _ = std::fs::remove_dir_all(&work);
    assert_eq!(matched, checked);
}
