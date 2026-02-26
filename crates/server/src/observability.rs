use std::time::Instant;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::alerts::AlertEvent;
use crate::state::AppState;

pub async fn track_requests(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let started = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.trim().is_empty())
        .map(|v| v.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut response = next.run(req).await;
    if let Ok(hv) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", hv);
    }

    if response.status().is_server_error() {
        let latency_ms = started.elapsed().as_millis() as u64;
        let status = response.status().as_u16();
        tracing::error!(
            request_id = %request_id,
            method = %method,
            path = %path,
            status = status,
            latency_ms = latency_ms,
            "HTTP 5xx response"
        );

        let alert = AlertEvent::http_5xx(request_id, method, path, status, latency_ms);
        state.alerts.emit(&alert).await;
    }

    response
}
