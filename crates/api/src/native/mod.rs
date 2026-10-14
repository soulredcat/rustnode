//! Native handlers: endpoints rustnode answers itself instead of the official
//! node. A handler returns `None` for any request it cannot answer exactly like
//! the official node; that request is then proxied.

mod address;

use crate::routes::Route;

/// Routes that have a native handler. Only these accept `shadow` and `native`.
#[must_use]
pub(crate) const fn has_handler(route: Route) -> bool {
    matches!(route, Route::AddressGroup)
}

/// The JSON body of a `200` response, or `None` to defer to the official node.
#[must_use]
pub(crate) fn serve(route: Route, path: &str, groups: u32) -> Option<Vec<u8>> {
    match route {
        Route::AddressGroup => {
            let address = path.split('/').nth(2)?;
            let group = address::asset_address_group(address, groups)?;
            Some(format!("{{\"group\":{group}}}").into_bytes())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{has_handler, serve};
    use crate::routes::Route;

    /// Group 2 with the 3 groups of the official `AddressSpec` vectors.
    #[test]
    fn address_group_body_matches_official_shape() {
        let body = serve(
            Route::AddressGroup,
            "/addresses/1C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3/group",
            3,
        );
        assert_eq!(body.as_deref(), Some(&b"{\"group\":2}"[..]));
    }

    #[test]
    fn routes_without_handlers_defer() {
        for route in Route::ALL {
            if !has_handler(route) {
                assert_eq!(serve(route, "/anything", 4), None, "{}", route.name());
            }
        }
        assert_eq!(serve(Route::AddressGroup, "/addresses/%31/group", 4), None);
    }
}
