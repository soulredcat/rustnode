//! The allowlist: every endpoint in `docs/NIXIA_API_SURFACE.md`, and nothing else.

use hyper::Method;

/// Longest path accepted; nixiaproject's longest is well under 200 bytes.
const MAX_PATH_BYTES: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Route {
    MempoolTransactions,
    ContractState,
    CallContract,
    CallTxScript,
    MulticallContract,
    ExecuteScript,
    DecodeUnsignedTx,
    BuildTransaction,
    SubmitTransaction,
    TransactionStatus,
    TransactionRichDetails,
    AddressBalance,
    AddressUtxos,
    AddressGroup,
    BlockByHash,
    BlocksByTime,
    BlockWithEvents,
    ContractEvents,
    ContractEventCount,
    EventsByTxId,
    SubContracts,
    InfosNode,
    WebSocket,
}

impl Route {
    pub const ALL: [Self; 23] = [
        Self::MempoolTransactions,
        Self::ContractState,
        Self::CallContract,
        Self::CallTxScript,
        Self::MulticallContract,
        Self::ExecuteScript,
        Self::DecodeUnsignedTx,
        Self::BuildTransaction,
        Self::SubmitTransaction,
        Self::TransactionStatus,
        Self::TransactionRichDetails,
        Self::AddressBalance,
        Self::AddressUtxos,
        Self::AddressGroup,
        Self::BlockByHash,
        Self::BlocksByTime,
        Self::BlockWithEvents,
        Self::ContractEvents,
        Self::ContractEventCount,
        Self::EventsByTxId,
        Self::SubContracts,
        Self::InfosNode,
        Self::WebSocket,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Short identifier used by `--route-mode`.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::MempoolTransactions => "mempool-transactions",
            Self::ContractState => "contract-state",
            Self::CallContract => "call-contract",
            Self::CallTxScript => "call-tx-script",
            Self::MulticallContract => "multicall-contract",
            Self::ExecuteScript => "execute-script",
            Self::DecodeUnsignedTx => "decode-unsigned-tx",
            Self::BuildTransaction => "build-transaction",
            Self::SubmitTransaction => "submit-transaction",
            Self::TransactionStatus => "transaction-status",
            Self::TransactionRichDetails => "transaction-rich-details",
            Self::AddressBalance => "address-balance",
            Self::AddressUtxos => "address-utxos",
            Self::AddressGroup => "address-group",
            Self::BlockByHash => "block-by-hash",
            Self::BlocksByTime => "blocks-by-time",
            Self::BlockWithEvents => "block-with-events",
            Self::ContractEvents => "contract-events",
            Self::ContractEventCount => "contract-event-count",
            Self::EventsByTxId => "events-by-tx-id",
            Self::SubContracts => "sub-contracts",
            Self::InfosNode => "infos-node",
            Self::WebSocket => "ws",
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MempoolTransactions => "GET /mempool/transactions",
            Self::ContractState => "GET /contracts/{address}/state",
            Self::CallContract => "POST /contracts/call-contract",
            Self::CallTxScript => "POST /contracts/call-tx-script",
            Self::MulticallContract => "POST /contracts/multicall-contract",
            Self::ExecuteScript => "POST /contracts/unsigned-tx/execute-script",
            Self::DecodeUnsignedTx => "POST /transactions/decode-unsigned-tx",
            Self::BuildTransaction => "POST /transactions/build",
            Self::SubmitTransaction => "POST /transactions/submit",
            Self::TransactionStatus => "GET /transactions/status",
            Self::TransactionRichDetails => "GET /transactions/rich-details/{txId}",
            Self::AddressBalance => "GET /addresses/{address}/balance",
            Self::AddressUtxos => "GET /addresses/{address}/utxos",
            Self::AddressGroup => "GET /addresses/{address}/group",
            Self::BlockByHash => "GET /blockflow/blocks/{hash}",
            Self::BlocksByTime => "GET /blockflow/blocks",
            Self::BlockWithEvents => "GET /blockflow/blocks-with-events/{hash}",
            Self::ContractEvents => "GET /events/contract/{address}",
            Self::ContractEventCount => "GET /events/contract/{address}/current-count",
            Self::EventsByTxId => "GET /events/tx-id/{txId}",
            Self::SubContracts => "GET /contracts/{address}/sub-contracts",
            Self::InfosNode => "GET /infos/node",
            Self::WebSocket => "GET /ws",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMatch {
    Found(Route),
    MethodNotAllowed,
    NotFound,
}

#[derive(Clone, Copy)]
enum Seg {
    Lit(&'static str),
    Param,
}

use Seg::{Lit, Param};

const TABLE: &[(Method, &[Seg], Route)] = &[
    (
        Method::GET,
        &[Lit("mempool"), Lit("transactions")],
        Route::MempoolTransactions,
    ),
    (
        Method::GET,
        &[Lit("contracts"), Param, Lit("state")],
        Route::ContractState,
    ),
    (
        Method::POST,
        &[Lit("contracts"), Lit("call-contract")],
        Route::CallContract,
    ),
    (
        Method::POST,
        &[Lit("contracts"), Lit("call-tx-script")],
        Route::CallTxScript,
    ),
    (
        Method::POST,
        &[Lit("contracts"), Lit("multicall-contract")],
        Route::MulticallContract,
    ),
    (
        Method::POST,
        &[Lit("contracts"), Lit("unsigned-tx"), Lit("execute-script")],
        Route::ExecuteScript,
    ),
    (
        Method::POST,
        &[Lit("transactions"), Lit("decode-unsigned-tx")],
        Route::DecodeUnsignedTx,
    ),
    (
        Method::POST,
        &[Lit("transactions"), Lit("build")],
        Route::BuildTransaction,
    ),
    (
        Method::POST,
        &[Lit("transactions"), Lit("submit")],
        Route::SubmitTransaction,
    ),
    (
        Method::GET,
        &[Lit("transactions"), Lit("status")],
        Route::TransactionStatus,
    ),
    (
        Method::GET,
        &[Lit("transactions"), Lit("rich-details"), Param],
        Route::TransactionRichDetails,
    ),
    (
        Method::GET,
        &[Lit("addresses"), Param, Lit("balance")],
        Route::AddressBalance,
    ),
    (
        Method::GET,
        &[Lit("addresses"), Param, Lit("utxos")],
        Route::AddressUtxos,
    ),
    (
        Method::GET,
        &[Lit("addresses"), Param, Lit("group")],
        Route::AddressGroup,
    ),
    (
        Method::GET,
        &[Lit("blockflow"), Lit("blocks"), Param],
        Route::BlockByHash,
    ),
    (
        Method::GET,
        &[Lit("blockflow"), Lit("blocks")],
        Route::BlocksByTime,
    ),
    (
        Method::GET,
        &[Lit("blockflow"), Lit("blocks-with-events"), Param],
        Route::BlockWithEvents,
    ),
    (
        Method::GET,
        &[Lit("events"), Lit("contract"), Param],
        Route::ContractEvents,
    ),
    (
        Method::GET,
        &[Lit("events"), Lit("contract"), Param, Lit("current-count")],
        Route::ContractEventCount,
    ),
    (
        Method::GET,
        &[Lit("events"), Lit("tx-id"), Param],
        Route::EventsByTxId,
    ),
    (
        Method::GET,
        &[Lit("contracts"), Param, Lit("sub-contracts")],
        Route::SubContracts,
    ),
    (Method::GET, &[Lit("infos"), Lit("node")], Route::InfosNode),
    (Method::GET, &[Lit("ws")], Route::WebSocket),
];

/// Matches a request against the allowlist. `path` excludes the query string.
#[must_use]
pub fn route(method: &Method, path: &str) -> RouteMatch {
    if path.len() > MAX_PATH_BYTES {
        return RouteMatch::NotFound;
    }
    let Some(rest) = path.strip_prefix('/') else {
        return RouteMatch::NotFound;
    };
    let mut segments = [""; 4];
    let mut count = 0;
    for segment in rest.split('/') {
        if segment.is_empty() || count == segments.len() {
            return RouteMatch::NotFound;
        }
        segments[count] = segment;
        count += 1;
    }
    let segments = &segments[..count];

    let mut path_known = false;
    for (allowed_method, pattern, route) in TABLE {
        if matches(pattern, segments) {
            if allowed_method == method {
                return RouteMatch::Found(*route);
            }
            path_known = true;
        }
    }
    if path_known {
        RouteMatch::MethodNotAllowed
    } else {
        RouteMatch::NotFound
    }
}

fn matches(pattern: &[Seg], segments: &[&str]) -> bool {
    pattern.len() == segments.len()
        && pattern
            .iter()
            .zip(segments)
            .all(|(expected, actual)| match expected {
                Lit(literal) => literal == actual,
                Param => true,
            })
}

#[cfg(test)]
mod tests {
    use hyper::Method;

    use super::{Route, RouteMatch, TABLE, route};

    const ADDRESS: &str = "1DrDyTr9RpRsQnDnXo2YRiPzPW4ooHX5LLoqXrqfMrpQH";
    const HASH: &str = "00000000000000a1c2b3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f6071829";

    #[test]
    fn every_surface_endpoint_is_found() {
        let cases = [
            (
                Method::GET,
                "/mempool/transactions".to_owned(),
                Route::MempoolTransactions,
            ),
            (
                Method::GET,
                format!("/contracts/{ADDRESS}/state"),
                Route::ContractState,
            ),
            (
                Method::POST,
                "/contracts/call-contract".to_owned(),
                Route::CallContract,
            ),
            (
                Method::POST,
                "/contracts/call-tx-script".to_owned(),
                Route::CallTxScript,
            ),
            (
                Method::POST,
                "/contracts/multicall-contract".to_owned(),
                Route::MulticallContract,
            ),
            (
                Method::POST,
                "/contracts/unsigned-tx/execute-script".to_owned(),
                Route::ExecuteScript,
            ),
            (
                Method::POST,
                "/transactions/decode-unsigned-tx".to_owned(),
                Route::DecodeUnsignedTx,
            ),
            (
                Method::POST,
                "/transactions/build".to_owned(),
                Route::BuildTransaction,
            ),
            (
                Method::POST,
                "/transactions/submit".to_owned(),
                Route::SubmitTransaction,
            ),
            (
                Method::GET,
                "/transactions/status".to_owned(),
                Route::TransactionStatus,
            ),
            (
                Method::GET,
                format!("/transactions/rich-details/{HASH}"),
                Route::TransactionRichDetails,
            ),
            (
                Method::GET,
                format!("/addresses/{ADDRESS}/balance"),
                Route::AddressBalance,
            ),
            (
                Method::GET,
                format!("/addresses/{ADDRESS}/utxos"),
                Route::AddressUtxos,
            ),
            (
                Method::GET,
                format!("/addresses/{ADDRESS}/group"),
                Route::AddressGroup,
            ),
            (
                Method::GET,
                format!("/blockflow/blocks/{HASH}"),
                Route::BlockByHash,
            ),
            (
                Method::GET,
                "/blockflow/blocks".to_owned(),
                Route::BlocksByTime,
            ),
            (
                Method::GET,
                format!("/blockflow/blocks-with-events/{HASH}"),
                Route::BlockWithEvents,
            ),
            (
                Method::GET,
                format!("/events/contract/{ADDRESS}"),
                Route::ContractEvents,
            ),
            (
                Method::GET,
                format!("/events/contract/{ADDRESS}/current-count"),
                Route::ContractEventCount,
            ),
            (
                Method::GET,
                format!("/events/tx-id/{HASH}"),
                Route::EventsByTxId,
            ),
            (
                Method::GET,
                format!("/contracts/{ADDRESS}/sub-contracts"),
                Route::SubContracts,
            ),
            (Method::GET, "/infos/node".to_owned(), Route::InfosNode),
            (Method::GET, "/ws".to_owned(), Route::WebSocket),
        ];
        assert_eq!(cases.len(), Route::ALL.len());
        assert_eq!(TABLE.len(), Route::ALL.len());
        for (method, path, expected) in cases {
            assert_eq!(
                route(&method, &path),
                RouteMatch::Found(expected),
                "{method} {path}"
            );
        }
    }

    #[test]
    fn route_ids_are_unique() {
        let mut ids: Vec<&str> = Route::ALL.iter().map(|route| route.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Route::ALL.len());
    }

    #[test]
    fn route_indexes_follow_all_order() {
        for (index, route) in Route::ALL.iter().enumerate() {
            assert_eq!(route.index(), index);
        }
    }

    #[test]
    fn wrong_method_on_known_path_is_rejected() {
        assert_eq!(
            route(&Method::GET, "/transactions/submit"),
            RouteMatch::MethodNotAllowed
        );
        assert_eq!(
            route(&Method::POST, "/mempool/transactions"),
            RouteMatch::MethodNotAllowed
        );
        assert_eq!(route(&Method::DELETE, "/ws"), RouteMatch::MethodNotAllowed);
    }

    #[test]
    fn endpoints_outside_the_surface_are_not_found() {
        for path in [
            "/",
            "",
            "/infos/self-clique",
            "/wallets",
            "/miners/cpu-mining",
            "/blockflow/chain-info",
            "/mempool/transactions/extra",
            "/mempool//transactions",
            "/mempool/transactions/",
            "mempool/transactions",
            "/a/b/c/d/e",
        ] {
            assert_eq!(route(&Method::GET, path), RouteMatch::NotFound, "{path:?}");
        }
        let long = format!("/contracts/{}/state", "x".repeat(4096));
        assert_eq!(route(&Method::GET, &long), RouteMatch::NotFound);
    }
}
