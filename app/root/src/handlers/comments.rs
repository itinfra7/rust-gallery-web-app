use axum::{
    extract::{Path as AxumPath, State, ConnectInfo},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::net::SocketAddr;
use sqlx::Row;
use chrono::Utc;

use crate::audit::{audit_actor_from_session, record_admin_audit};
use crate::state::AppState;
use crate::auth::check_session;
use crate::handlers::dashboard::{publish_image_activity_event, push_dashboard_alert};
use crate::rate_limit::COMMENT_ACTION_POLICY;
use super::common::{build_rate_limit_key, get_client_ip, retry_after_headers};
use super::user_utils::get_or_create_user;

#[derive(Deserialize)]
pub struct CreateCommentPayload {
    pub content: String,
    pub fingerprint: String,
}

#[derive(Serialize)]
pub struct CommentResponse {
    pub id: String,
    pub content: String,
    pub created_at: String,
    pub ip_masked: String,
    pub browser: String,
    pub fingerprint_short: String,
}

pub async fn get_comments_handler(
    State(state): State<AppState>,
    AxumPath(image_id): AxumPath<Uuid>,
) -> impl IntoResponse {
    let rows = sqlx::query(
        r#"
        SELECT c.id, c.content, c.created_at, c.user_agent, u.ip_address, u.fingerprint
        FROM comments c
        JOIN users u ON c.user_id = u.id
        WHERE c.image_id = $1
        ORDER BY c.created_at ASC
        "#
    )
    .bind(image_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let comments: Vec<CommentResponse> = rows.into_iter().map(|row| {
        let ip: String = row.get("ip_address");
        let ua: String = row.get("user_agent");
        let fp: String = row.get("fingerprint");
        let created: chrono::DateTime<Utc> = row.get("created_at");

        let ip_parts: Vec<&str> = ip.split('.').collect();
        let ip_masked = if ip_parts.len() == 4 {
            format!("{}.{}.***.***", ip_parts[0], ip_parts[1])
        } else {
            "***".to_string()
        };

        let browser = if ua.contains("Chrome") { "Chrome" }
        else if ua.contains("Firefox") { "Firefox" }
        else if ua.contains("Safari") { "Safari" }
        else if ua.contains("Edge") { "Edge" }
        else { "Unknown" }.to_string();

        let fp_len = fp.len();
        let fp_short = if fp_len > 4 {
            fp[fp_len - 4..].to_string()
        } else {
            fp
        };

        CommentResponse {
            id: row.get::<Uuid, _>("id").to_string(),
            content: row.get("content"),
            created_at: created.format("%Y-%m-%d %H:%M:%S").to_string(),
            ip_masked,
            browser,
            fingerprint_short: fp_short,
        }
    }).collect();

    Json(comments)
}

pub async fn add_comment_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    AxumPath(image_id): AxumPath<Uuid>,
    Json(payload): Json<CreateCommentPayload>,
) -> impl IntoResponse {
    if payload.content.len() > 100 || payload.content.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "error_content_length").into_response();
    }

    let client_ip = get_client_ip(&headers, Some(addr));
    let rate_limit_key = build_rate_limit_key("comment", &client_ip, Some(&payload.fingerprint));
    let rate_limit = state.record_rate_limit_event(&rate_limit_key, COMMENT_ACTION_POLICY);
    if !rate_limit.allowed {
        if rate_limit.just_blocked {
            push_dashboard_alert(
                &state,
                "warn",
                "rate-limit",
                "comment_rate_limited",
                "Comment throttled",
                format!("Blocked repeated comments from {}", client_ip),
            );
        }

        return (
            StatusCode::TOO_MANY_REQUESTS,
            retry_after_headers(rate_limit.retry_after_seconds),
            "error_comment_rate_limited",
        )
            .into_response();
    }

    let user_agent = headers.get("User-Agent")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("Unknown")
        .to_string();

    let user_id = get_or_create_user(&state.db, &client_ip, &payload.fingerprint)
        .await
        .unwrap();

    let count_check: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM comments
        WHERE image_id = $1 AND user_id = $2
        "#
    )
    .bind(image_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    if count_check >= 3 {
        return (StatusCode::FORBIDDEN, "error_comment_image_limit").into_response();
    }

    let comment_content = payload.content.trim().to_string();

    sqlx::query(
        "INSERT INTO comments (id, image_id, user_id, content, user_agent) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(Uuid::new_v4())
    .bind(image_id)
    .bind(user_id)
    .bind(&comment_content)
    .bind(user_agent)
    .execute(&state.db)
    .await
    .unwrap();

    publish_image_activity_event(
        &state,
        image_id,
        "comment",
        "New comment",
        Some(comment_content),
    )
    .await;

    (StatusCode::CREATED, "success_comment_added").into_response()
}

pub async fn delete_comment_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    AxumPath(comment_id): AxumPath<Uuid>,
) -> impl IntoResponse {
    if !check_session(&state, &jar) {
        return (StatusCode::UNAUTHORIZED, "error_unauthorized").into_response();
    }

    let source_ip = get_client_ip(&headers, Some(addr));
    let actor = audit_actor_from_session(&state, &jar);

    let linked_image_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT image_id FROM comments WHERE id = $1"
    )
    .bind(comment_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    sqlx::query("DELETE FROM comments WHERE id = $1")
        .bind(comment_id)
        .execute(&state.db)
        .await
        .unwrap();

    if let Some(image_id) = linked_image_id {
        publish_image_activity_event(
            &state,
            image_id,
            "admin",
            "Comment deleted",
            None,
        )
        .await;
    }

    record_admin_audit(
        actor,
        "delete_comment",
        "success",
        &source_ip,
        &format!("/api/comment/{}", comment_id),
        format!("Deleted comment {}", comment_id),
    );

    (StatusCode::OK, "success_comment_deleted").into_response()
}
