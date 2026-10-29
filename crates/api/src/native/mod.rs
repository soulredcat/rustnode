//! Native handlers: endpoints rustnode answers itself instead of the official
//! node. A handler returns `None` for any request it cannot answer exactly like
//! the official node; that request is then proxied.

mod address;
mod mempool;
mod status;

use std::sync::PoisonError;

use alephium_chain::SharedChainStore;
use alephium_flow::SharedMempool;
use alephium_model::Hash;

use crate::routes::Route;

/// What native handlers may read.
pub(crate) struct Context<'a> {
    pub(crate) groups: u32,
    pub(crate) chain: Option<&'a SharedChainStore>,
    pub(crate) mempool: Option<&'a SharedMempool>,
}

/// Routes that have a native handler. Only these accept `shadow` and `native`.
#[must_use]
pub(crate) const fn has_handler(route: Route) -> bool {
    matches!(
        route,
        Route::AddressGroup
            | Route::BlockByHash
            | Route::MempoolTransactions
            | Route::TransactionStatus
    )
}

/// Handlers that matched the official node on live traffic
/// (`docs/MAINNET_CHECK.md`) and therefore run `native` by default. A new
/// handler starts in `shadow` and is added here once it has that evidence.
#[must_use]
pub(crate) const fn verified(route: Route) -> bool {
    matches!(route, Route::AddressGroup)
}

/// The JSON body of a `200` response, or `None` to defer to the official node.
#[must_use]
pub(crate) fn serve(
    route: Route,
    path: &str,
    query: Option<&str>,
    context: &Context<'_>,
) -> Option<Vec<u8>> {
    match route {
        Route::AddressGroup => {
            let address = path.split('/').nth(2)?;
            let group = address::asset_address_group(address, context.groups)?;
            Some(format!("{{\"group\":{group}}}").into_bytes())
        }
        Route::BlockByHash => block_by_hash(path.split('/').nth(3)?, context),
        Route::MempoolTransactions if query.is_none() => mempool::transactions(context),
        Route::TransactionStatus => status::transaction_status(query, context),
        _ => None,
    }
}

/// A followed block. Inter-group blocks are deferred: since Danube their
/// `conflictedTxs` depend on the asset UTXO set, which is not tracked yet.
fn block_by_hash(hash: &str, context: &Context<'_>) -> Option<Vec<u8>> {
    let hash = Hash::from_hex(hash)?;
    let store = context
        .chain?
        .read()
        .unwrap_or_else(PoisonError::into_inner);
    let entry = store.as_ref()?.get(&hash)?;
    if entry.from != entry.to {
        return None;
    }
    let json = crate::json::block_entry(&entry.block, entry.height, context.groups, None)?;
    Some(json.to_string().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::{Context, has_handler, serve};
    use crate::routes::Route;

    /// Group 2 with the 3 groups of the official `AddressSpec` vectors.
    fn context(groups: u32) -> Context<'static> {
        Context {
            groups,
            chain: None,
            mempool: None,
        }
    }

    #[test]
    fn address_group_body_matches_official_shape() {
        let body = serve(
            Route::AddressGroup,
            "/addresses/1C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3/group",
            None,
            &context(3),
        );
        assert_eq!(body.as_deref(), Some(&b"{\"group\":2}"[..]));
    }

    #[test]
    fn routes_without_handlers_defer() {
        for route in Route::ALL {
            if !has_handler(route) {
                assert_eq!(
                    serve(route, "/anything", None, &context(4)),
                    None,
                    "{}",
                    route.name()
                );
            }
        }
        assert_eq!(
            serve(
                Route::AddressGroup,
                "/addresses/%31/group",
                None,
                &context(4)
            ),
            None
        );
        // Without a followed chain, blocks, statuses and the mempool are
        // always deferred.
        assert_eq!(
            serve(
                Route::MempoolTransactions,
                "/mempool/transactions",
                None,
                &context(4)
            ),
            None
        );
        assert_eq!(
            serve(
                Route::TransactionStatus,
                "/transactions/status",
                Some(&format!("txId={}", "00".repeat(32))),
                &context(4)
            ),
            None
        );
        let hash = "00".repeat(32);
        assert_eq!(
            serve(
                Route::BlockByHash,
                &format!("/blockflow/blocks/{hash}"),
                None,
                &context(4)
            ),
            None
        );
    }
}
