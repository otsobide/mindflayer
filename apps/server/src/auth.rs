//! Who may use the API: whoever holds the launch token.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, COOKIE, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::api;
use crate::token::Token;

/// The check every `/api` request passes before a handler sees it.
pub(crate) struct Gate {
    token: Token,
    /// The cookie the token travels in: `mf_<port>`.
    cookie: String,
}

impl Gate {
    pub(crate) fn new(token: Token, port: u16) -> Self {
        Self {
            token,
            cookie: cookie_name(port),
        }
    }

    /// Whether a request carries the token, as a cookie (the panel) or as a
    /// bearer token (scripts and tests, which have no cookie jar to speak of).
    pub(crate) fn admits(&self, headers: &HeaderMap) -> bool {
        self.bearer(headers) || self.cookie(headers)
    }

    fn bearer(&self, headers: &HeaderMap) -> bool {
        headers
            .get_all(AUTHORIZATION)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| value.split_once(' '))
            // The scheme is case-insensitive (RFC 9110), the token is not.
            .any(|(scheme, offered)| {
                scheme.eq_ignore_ascii_case("bearer") && self.token.matches(offered.trim())
            })
    }

    fn cookie(&self, headers: &HeaderMap) -> bool {
        headers
            .get_all(COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .any(|(name, offered)| name == self.cookie && self.token.matches(offered))
    }
}

/// The cookie's name on a given port.
///
/// Per port because cookies are not: a browser sends a cookie set by
/// 127.0.0.1:4000 to 127.0.0.1:5000 as well. Under one shared name, two
/// panels open at once would each overwrite the other's token, and the first
/// would be locked out by the second's launch.
pub(crate) fn cookie_name(port: u16) -> String {
    format!("mf_{port}")
}

/// Turn away a request without the token, before it reaches a handler.
pub(crate) async fn require(
    State(gate): State<Arc<Gate>>,
    request: Request,
    next: Next,
) -> Response {
    if gate.admits(request.headers()) {
        next.run(request).await
    } else {
        let mut response = api::error(
            StatusCode::UNAUTHORIZED,
            "this needs the launch token, as the cookie its link sets or as a Bearer token",
        );
        response
            .headers_mut()
            .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        response
    }
}
