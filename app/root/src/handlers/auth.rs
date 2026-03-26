use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{Duration as ChronoDuration, Utc};
use time::Duration as CookieDuration;
use uuid::Uuid;
use std::net::SocketAddr;
use std::time::Duration;

use crate::{
    audit::record_admin_audit,
    auth::{session_user_id, verify_user},
    handlers::dashboard::push_dashboard_alert,
    rate_limit::LOGIN_FAILURE_POLICY,
    state::{AppState, Session},
};
use super::common::{build_rate_limit_key, get_client_ip, retry_after_headers, LoginPayload};

pub async fn login_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    Json(payload): Json<LoginPayload>,
) -> impl IntoResponse {
    let source_ip = get_client_ip(&headers, Some(addr));
    let limit_key = build_rate_limit_key("login", &source_ip, None);
    let precheck = state.check_rate_limit(&limit_key, LOGIN_FAILURE_POLICY);

    if !precheck.allowed {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            retry_after_headers(precheck.retry_after_seconds),
            Json(serde_json::json!({
                "success": false,
                "error": "error_login_rate_limited",
                "retry_after_seconds": precheck.retry_after_seconds
            })),
        )
            .into_response();
    }

    if verify_user(&payload.id, &payload.password, &payload.key) {
        let actor = payload.id.clone();
        let session_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let session = Session {
            user_id: payload.id,
            created_at: now,
            expires_at: now + ChronoDuration::hours(6),
        };

        state.reset_rate_limit(&limit_key);
        state.insert_session(session_id.clone(), session);

        let cookie = Cookie::build(("session_id", session_id))
            .path("/")
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Strict)
            .max_age(CookieDuration::hours(6))
            .build();

        record_admin_audit(
            actor,
            "login",
            "success",
            &source_ip,
            "/api/login",
            "Admin login accepted",
        );

        return (jar.add(cookie), Json(serde_json::json!({ "success": true }))).into_response();
    }

    let failed = state.record_rate_limit_event(&limit_key, LOGIN_FAILURE_POLICY);

    record_admin_audit(
        if payload.id.trim().is_empty() {
            "unknown".to_string()
        } else {
            payload.id
        },
        "login",
        "failed",
        &source_ip,
        "/api/login",
        "Invalid admin credentials",
    );

    if failed.just_blocked {
        push_dashboard_alert(
            &state,
            "warn",
            "rate-limit",
            "login_bruteforce_blocked",
            "Login throttled",
            format!("Blocked repeated admin login failures from {}", source_ip),
        );
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    if !failed.allowed {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            retry_after_headers(failed.retry_after_seconds),
            Json(serde_json::json!({
                "success": false,
                "error": "error_login_rate_limited",
                "retry_after_seconds": failed.retry_after_seconds
            })),
        )
            .into_response();
    }

    (jar, Json(serde_json::json!({ "success": false }))).into_response()
}

pub async fn logout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    jar: CookieJar,
) -> impl IntoResponse {
    let source_ip = get_client_ip(&headers, Some(addr));
    let actor = session_user_id(&state, &jar).unwrap_or_else(|| "unknown".to_string());

    if let Some(cookie) = jar.get("session_id") {
        state.remove_session(cookie.value());
    }

    record_admin_audit(
        actor,
        "logout",
        "success",
        &source_ip,
        "/api/logout",
        "Admin logout completed",
    );

    let removal_cookie = Cookie::build(("session_id", ""))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Strict)
        .max_age(CookieDuration::seconds(0))
        .build();

    (
        jar.remove(removal_cookie),
        Json(serde_json::json!({ "success": true })),
    )
}
