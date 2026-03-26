use axum::{
    extract::{ConnectInfo, State, Path as AxumPath},
    http::HeaderMap,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::{fs, path::{Path, PathBuf}};
use sqlx::Row;

use crate::audit::{audit_actor_from_session, record_admin_audit};
use crate::state::AppState;
use crate::auth::check_session;
use crate::handlers::dashboard::{
    build_image_paths, publish_dashboard_event, push_dashboard_alert,
};
use crate::handlers::seo::{indexnow_home_url, indexnow_image_url, indexnow_tag_url, spawn_indexnow_submission};
use super::common::{get_client_ip, UpdateTagsPayload, BulkDeletePayload, summarize_tags};

pub async fn update_tags_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    AxumPath(id): AxumPath<Uuid>,
    Json(payload): Json<UpdateTagsPayload>,
) -> impl IntoResponse {
    if !check_session(&state, &jar) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let source_ip = get_client_ip(&headers, Some(addr));
    let actor = audit_actor_from_session(&state, &jar);

    let tags_vec: Vec<String> = payload.tags
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let (existing_tags, upload_date) = match sqlx::query("SELECT tags, upload_date FROM images WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(row)) => (
            row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default(),
            row.get::<DateTime<Utc>, _>("upload_date"),
        ),
        Ok(None) => return (StatusCode::NOT_FOUND, "Not Found").into_response(),
        Err(_) => {
            push_dashboard_alert(
                &state,
                "error",
                "admin",
                "tag_update_lookup_failed",
                "Tag update lookup failed",
                format!("Failed to load tag state for image {}", id),
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, "DB Error").into_response();
        }
    };

    if let Err(_) = sqlx::query("UPDATE images SET tags = $1 WHERE id = $2")
        .bind(&tags_vec)
        .bind(id)
        .execute(&state.db)
        .await
    {
        push_dashboard_alert(
            &state,
            "error",
            "admin",
            "tag_update_failed",
            "Tag update failed",
            format!("Failed to update tags for image {}", id),
        );
        return (StatusCode::INTERNAL_SERVER_ERROR, "DB Error").into_response();
    }

    let upload_date_str = upload_date.format("%Y-%m-%d").to_string();
    let mut changed_urls = vec![
        indexnow_home_url(),
        indexnow_image_url(id, &existing_tags, &upload_date_str),
        indexnow_image_url(id, &tags_vec, &upload_date_str),
    ];
    for tag in existing_tags.iter().chain(tags_vec.iter()) {
        changed_urls.push(indexnow_tag_url(tag));
    }

    let (page_url, thumb_url, title, _) = build_image_paths(id, &tags_vec, upload_date);
    let tag_summary = if tags_vec.is_empty() {
        "No tags".to_string()
    } else {
        summarize_tags(&tags_vec, 8)
    };
    publish_dashboard_event(
        &state,
        "admin",
        "Tags updated",
        format!("{} · {}", title, tag_summary),
        page_url,
        thumb_url,
    );

    record_admin_audit(
        actor,
        "update_tags",
        "success",
        &source_ip,
        &format!("/api/image/{}/tags", id),
        format!("{} tags on image {}", tags_vec.len(), id),
    );

    spawn_indexnow_submission(state.clone(), changed_urls);
    (StatusCode::OK, "Updated").into_response()
}

pub async fn delete_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    AxumPath(filename): AxumPath<String>,
) -> impl IntoResponse {
    if !check_session(&state, &jar) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let source_ip = get_client_ip(&headers, Some(addr));
    let actor = audit_actor_from_session(&state, &jar);

    let clean_name = Path::new(&filename).file_name().and_then(|n| n.to_str()).unwrap_or("");
    if clean_name.is_empty() || clean_name.contains("..") {
        return (StatusCode::BAD_REQUEST, "Invalid Filename").into_response();
    }

    let original_path = PathBuf::from("uploads").join(clean_name);
    let thumb_name = clean_name.replace(".webp", "_thumb.webp");
    let thumb_path = PathBuf::from("uploads/thumbs").join(thumb_name);

    let (deleted_id, deleted_tags, upload_date) = match sqlx::query(
        "DELETE FROM images WHERE filename = $1 RETURNING id, tags, upload_date"
    )
        .bind(clean_name)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(row)) => (
            row.get::<Uuid, _>("id"),
            row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default(),
            row.get::<DateTime<Utc>, _>("upload_date"),
        ),
        Ok(None) => return (StatusCode::NOT_FOUND, "Not Found").into_response(),
        Err(_) => {
            push_dashboard_alert(
                &state,
                "error",
                "admin",
                "delete_failed",
                "Delete failed",
                format!("Database delete failed for {}", clean_name),
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, "DB Error").into_response();
        }
    };

    if original_path.exists() {
        fs::remove_file(original_path).ok();
    }
    if thumb_path.exists() {
        fs::remove_file(thumb_path).ok();
    }

    let upload_date_str = upload_date.format("%Y-%m-%d").to_string();
    let mut changed_urls = vec![
        indexnow_home_url(),
        indexnow_image_url(deleted_id, &deleted_tags, &upload_date_str),
    ];
    for tag in deleted_tags {
        changed_urls.push(indexnow_tag_url(&tag));
    }

    publish_dashboard_event(
        &state,
        "admin",
        "Image deleted",
        format!("Deleted {}", clean_name),
        "/",
        "",
    );

    record_admin_audit(
        actor,
        "delete_image",
        "success",
        &source_ip,
        &format!("/api/delete/{}", clean_name),
        format!("Deleted {}", clean_name),
    );

    spawn_indexnow_submission(state.clone(), changed_urls);
    (StatusCode::OK, "Deleted").into_response()
}

pub async fn bulk_delete_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(payload): Json<BulkDeletePayload>,
) -> impl IntoResponse {
    if !check_session(&state, &jar) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let source_ip = get_client_ip(&headers, Some(addr));
    let actor = audit_actor_from_session(&state, &jar);
    let requested_count = payload.filenames.len();

    let mut changed_urls = Vec::new();
    let mut deleted_count = 0usize;

    for filename in payload.filenames {
        let clean_name = Path::new(&filename).file_name().and_then(|n| n.to_str()).unwrap_or("");
        if clean_name.is_empty() || clean_name.contains("..") {
            continue;
        }

        let (deleted_id, deleted_tags, upload_date) = match sqlx::query(
            "DELETE FROM images WHERE filename = $1 RETURNING id, tags, upload_date"
        )
            .bind(clean_name)
            .fetch_optional(&state.db)
            .await
        {
            Ok(Some(row)) => (
                row.get::<Uuid, _>("id"),
                row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default(),
                row.get::<DateTime<Utc>, _>("upload_date"),
            ),
            Ok(None) => continue,
            Err(_) => continue,
        };

        let original_path = PathBuf::from("uploads").join(clean_name);
        let thumb_name = clean_name.replace(".webp", "_thumb.webp");
        let thumb_path = PathBuf::from("uploads/thumbs").join(thumb_name);

        if original_path.exists() {
            fs::remove_file(original_path).ok();
        }
        if thumb_path.exists() {
            fs::remove_file(thumb_path).ok();
        }

        let upload_date_str = upload_date.format("%Y-%m-%d").to_string();
        changed_urls.push(indexnow_image_url(deleted_id, &deleted_tags, &upload_date_str));
        for tag in deleted_tags {
            changed_urls.push(indexnow_tag_url(&tag));
        }
        deleted_count += 1;
    }

    if !changed_urls.is_empty() {
        changed_urls.push(indexnow_home_url());
        publish_dashboard_event(
            &state,
            "admin",
            "Bulk delete",
            format!("Deleted {} images", deleted_count),
            "/",
            "",
        );
        spawn_indexnow_submission(state.clone(), changed_urls);
    }

    record_admin_audit(
        actor,
        "bulk_delete",
        "success",
        &source_ip,
        "/api/delete/bulk",
        format!("Requested {} files, deleted {}", requested_count, deleted_count),
    );

    (StatusCode::OK, "Deleted").into_response()
}
