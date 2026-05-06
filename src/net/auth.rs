//! Authentication middleware for axum
//!
//! Extracts bearer tokens and access tokens from requests,
//! builds RequestContext, and enforces authorization.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::auth::authorizer::{AuthorizationAction, RequestContext};

use super::state::AppState;

/// Extract bearer token from Authorization header or access_token query param.
pub fn extract_auth_token(headers: &HeaderMap, uri: &str) -> String {
    // Try Authorization header first
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(value) = auth_header.to_str() {
            if let Some(token) = value.strip_prefix("Bearer ") {
                return token.to_string();
            }
            // Some clients send just the token
            return value.to_string();
        }
    }

    // Try access_token query parameter
    if let Some(query) = uri.split('?').nth(1) {
        for pair in query.split('&') {
            if let Some(token) = pair.strip_prefix("access_token=") {
                return token.to_string();
            }
        }
    }

    String::new()
}

/// Determine if a request is from a local address.
pub fn is_local_request(client_addr: &str) -> bool {
    client_addr.starts_with("127.0.0.1")
        || client_addr.starts_with("::1")
        || client_addr.starts_with("localhost")
        || client_addr.starts_with("[::1]")
}

/// Build a RequestContext from an HTTP request.
pub fn build_request_context(req: &Request, client_addr: &str) -> RequestContext {
    let token = extract_auth_token(req.headers(), req.uri().path());
    RequestContext {
        bearer_token: token,
        client_address: client_addr.to_string(),
        target: req.uri().path().to_string(),
        is_websocket: false,
        is_local_request: is_local_request(client_addr),
    }
}

/// Middleware that requires authentication for a request.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let client_addr = extract_client_addr(&req);
    let context = build_request_context(&req, &client_addr);

    let auth_result = state.authorizer.authenticate_bearer_token(&context.bearer_token);

    if !auth_result.authenticated {
        // Allow local requests through for certain paths
        if context.is_local_request && is_admin_path(req.uri().path()) {
            // Inject context for handlers
            req.extensions_mut().insert(context);
            return next.run(req).await;
        }

        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    req.extensions_mut().insert(context);
    next.run(req).await
}

/// Middleware that checks authorization for a specific action.
pub async fn require_authorization(
    state: &AppState,
    context: &RequestContext,
    action: AuthorizationAction,
) -> Result<(), Response> {
    let result = state.authorizer.authorize(context, action);
    if !result.authorized {
        Err((StatusCode::FORBIDDEN, "Forbidden").into_response())
    } else {
        Ok(())
    }
}

/// Extract client address from request extensions or headers.
fn extract_client_addr(req: &Request) -> String {
    // Try X-Forwarded-For header
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(value) = forwarded.to_str() {
            if let Some(addr) = value.split(',').next() {
                return addr.trim().to_string();
            }
        }
    }

    // Try X-Real-IP header
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            return value.to_string();
        }
    }

    // Default to unknown
    "unknown".to_string()
}

/// Check if a path is an admin-only path.
fn is_admin_path(path: &str) -> bool {
    path.starts_with("/host/") || path == "/host"
}

/// Get the RequestContext from request extensions.
pub fn get_request_context(req: &Request) -> Option<&RequestContext> {
    req.extensions().get::<RequestContext>()
}
