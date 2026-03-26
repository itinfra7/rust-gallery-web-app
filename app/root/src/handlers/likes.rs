use axum::{
    extract::{State, Path as AxumPath, ConnectInfo},
    http::HeaderMap,
    response::{IntoResponse, Json},
};
use std::net::SocketAddr;
use uuid::Uuid;
use sqlx::Row;
use serde::Deserialize;

use crate::state::AppState;
use crate::handlers::dashboard::{publish_image_activity_event, push_dashboard_alert};
use crate::rate_limit::LIKE_ACTION_POLICY;
use super::common::{build_rate_limit_key, get_client_ip, retry_after_headers};
use super::user_utils::get_or_create_user;

#[derive(Deserialize)]
pub struct ToggleLikePayload {
    pub fingerprint: String,
}

#[derive(Deserialize)]
pub struct CheckLikesPayload {
    pub fingerprint: String,
    pub image_ids: Vec<Uuid>,
}

pub async fn toggle_like_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    AxumPath(id): AxumPath<Uuid>,
    Json(payload): Json<ToggleLikePayload>,
) -> impl IntoResponse {
    let client_ip = get_client_ip(&headers, Some(addr));
    let rate_limit_key = build_rate_limit_key("like", &client_ip, Some(&payload.fingerprint));
    let rate_limit = state.record_rate_limit_event(&rate_limit_key, LIKE_ACTION_POLICY);

    if !rate_limit.allowed {
        if rate_limit.just_blocked {
            push_dashboard_alert(
                &state,
                "warn",
                "rate-limit",
                "like_rate_limited",
                "Like throttled",
                format!("Blocked repeated likes from {}", client_ip),
            );
        }

        return (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            retry_after_headers(rate_limit.retry_after_seconds),
            Json(serde_json::json!({
                "error": "error_like_rate_limited",
                "retry_after_seconds": rate_limit.retry_after_seconds
            })),
        )
            .into_response();
    }
    
    let user_id = get_or_create_user(&state.db, &client_ip, &payload.fingerprint)
        .await
        .unwrap();

    let mut tx = state.db.begin().await.unwrap();

    let existing = sqlx::query("SELECT 1 FROM likes WHERE image_id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .unwrap();

    let (liked, count) = if existing.is_some() {
        sqlx::query("DELETE FROM likes WHERE image_id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .unwrap();

        let row = sqlx::query("UPDATE images SET like_count = like_count - 1 WHERE id = $1 RETURNING like_count")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();

        (false, row.get::<i32, _>("like_count"))
    } else {
        sqlx::query("INSERT INTO likes (image_id, user_id) VALUES ($1, $2)")
            .bind(id)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .unwrap();

        let row = sqlx::query("UPDATE images SET like_count = like_count + 1 WHERE id = $1 RETURNING like_count")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();

        (true, row.get::<i32, _>("like_count"))
    };

    tx.commit().await.unwrap();

    if liked {
        publish_image_activity_event(&state, id, "like", "New like", None).await;
    } else {
        publish_image_activity_event(&state, id, "unlike", "Like removed", None).await;
    }

    Json(serde_json::json!({ "liked": liked, "count": count })).into_response()
}

pub async fn check_likes_handler(
    State(state): State<AppState>,
    Json(payload): Json<CheckLikesPayload>,
) -> impl IntoResponse {
    let rows = sqlx::query(
        r#"
        SELECT l.image_id
        FROM likes l
        JOIN users u ON l.user_id = u.id
        WHERE u.fingerprint = $1 AND l.image_id = ANY($2)
        "#
    )
    .bind(payload.fingerprint)
    .bind(payload.image_ids)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let liked_ids: Vec<Uuid> = rows.into_iter().map(|row| row.get("image_id")).collect();
    Json(liked_ids)
}
