use axum::{
    extract::{ConnectInfo, Multipart, State},
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Json},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use std::net::SocketAddr;
use std::{fs, path::Path};
use crate::audit::record_admin_audit;
use crate::auth::{check_session, session_user_id};
use crate::state::AppState;
use crate::handlers::seo::{indexnow_home_url, indexnow_image_url, spawn_indexnow_submission};
use crate::handlers::common::get_client_ip;
use crate::handlers::dashboard::{
    build_image_paths, publish_image_activity_event, push_dashboard_alert,
};
use crate::state::DashboardUploadJob;
use crate::utils::generate_complex_filename;
use super::stream::save_multipart_to_temp;
use super::processor::process_image;

fn phase_from_status(message: &str) -> &'static str {
    if message.contains("Receiving") {
        "receiving"
    } else if message.contains("Decoding") {
        "decoding"
    } else if message.contains("Encoding") {
        "encoding"
    } else if message.contains("Saving DB") {
        "persisting"
    } else if message.contains("Processing") {
        "processing"
    } else if message.contains("completed") {
        "completed"
    } else if message.contains("Failed") {
        "failed"
    } else {
        "processing"
    }
}

fn upsert_upload_job(
    state: &AppState,
    upload_id: &str,
    file_name: &str,
    state_name: &str,
    message: &str,
    started_at: &str,
    page_url: &str,
    image_id: &str,
) {
    state.upsert_upload_job(DashboardUploadJob {
        upload_id: upload_id.to_string(),
        file_name: file_name.to_string(),
        state: state_name.to_string(),
        phase: phase_from_status(message).to_string(),
        message: message.to_string(),
        started_at: started_at.to_string(),
        updated_at: Utc::now().to_rfc3339(),
        page_url: page_url.to_string(),
        image_id: image_id.to_string(),
    });
}

pub async fn get_upload_status_handler(
    State(state): State<AppState>,
    axum::extract::Path(upload_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let status = state.upload_status.lock().unwrap()
        .get(&upload_id).cloned().unwrap_or_else(|| "Waiting...".to_string());
    Json(serde_json::json!({ "status": status }))
}

pub async fn upload_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if !check_session(&state, &jar) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let source_ip = get_client_ip(&headers, Some(addr));
    let actor = session_user_id(&state, &jar).unwrap_or_else(|| "unknown".to_string());

    let upload_id = headers.get("X-Upload-ID")
        .and_then(|v| v.to_str().ok()).unwrap_or("unknown").to_string();
    let started_at = Utc::now().to_rfc3339();

    let mut changed_urls = Vec::new();
    let mut processed_anything = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        let original_file_name = field.file_name().unwrap_or("unknown").to_string();
        let content_type = field.content_type().unwrap_or("").to_string();

        let initial_message = "Receiving Stream...";
        state
            .upload_status
            .lock()
            .unwrap()
            .insert(upload_id.clone(), initial_message.to_string());
        upsert_upload_job(
            &state,
            &upload_id,
            &original_file_name,
            "running",
            initial_message,
            &started_at,
            "",
            "",
        );

        let temp_name = generate_complex_filename();
        let temp_path = Path::new("uploads").join(format!("temp_{}", temp_name));

        let save_result = save_multipart_to_temp(field, &temp_path).await;
        if let Err(error) = save_result {
            if temp_path.exists() {
                fs::remove_file(&temp_path).ok();
            }
            let failure_message = format!("Failed to receive upload stream: {}", error);
            upsert_upload_job(
                &state,
                &upload_id,
                &original_file_name,
                "failed",
                &failure_message,
                &started_at,
                "",
                "",
            );
            push_dashboard_alert(
                &state,
                "error",
                "upload",
                "upload_stream_failed",
                "Upload stream failed",
                failure_message,
            );
            record_admin_audit(
                actor.clone(),
                "upload",
                "failed",
                &source_ip,
                "/api/upload",
                format!("{} · stream receive failed", original_file_name),
            );
            continue;
        }

        let update_status = |msg: &str| {
            state
                .upload_status
                .lock()
                .unwrap()
                .insert(upload_id.clone(), msg.to_string());
            upsert_upload_job(
                &state,
                &upload_id,
                &original_file_name,
                "running",
                msg,
                &started_at,
                "",
                "",
            );
        };

        update_status("Processing...");
        match process_image(&state, &temp_path, &content_type, &update_status).await {
            Ok(Some((image_id, upload_date))) => {
                processed_anything = true;
                let upload_date_str = upload_date.format("%Y-%m-%d").to_string();
                let empty_tags = Vec::new();
                let (page_url, _, _, _) = build_image_paths(image_id, &empty_tags, upload_date);
                changed_urls.push(indexnow_image_url(image_id, &empty_tags, &upload_date_str));
                upsert_upload_job(
                    &state,
                    &upload_id,
                    &original_file_name,
                    "completed",
                    "Upload completed",
                    &started_at,
                    &page_url,
                    &image_id.to_string(),
                );
                publish_image_activity_event(&state, image_id, "upload", "New upload", None).await;
                record_admin_audit(
                    actor.clone(),
                    "upload",
                    "success",
                    &source_ip,
                    "/api/upload",
                    format!("{} -> {}", original_file_name, image_id),
                );
            }
            Ok(None) => {
                let failure_message = "Upload failed: image could not be decoded or encoded.";
                upsert_upload_job(
                    &state,
                    &upload_id,
                    &original_file_name,
                    "failed",
                    failure_message,
                    &started_at,
                    "",
                    "",
                );
                push_dashboard_alert(
                    &state,
                    "warn",
                    "upload",
                    "upload_processing_no_output",
                    "Upload processing returned no output",
                    failure_message,
                );
                record_admin_audit(
                    actor.clone(),
                    "upload",
                    "failed",
                    &source_ip,
                    "/api/upload",
                    format!("{} · decode or encode returned no output", original_file_name),
                );
            }
            Err(error) => {
                let failure_message = format!("Upload processing failed: {}", error);
                upsert_upload_job(
                    &state,
                    &upload_id,
                    &original_file_name,
                    "failed",
                    &failure_message,
                    &started_at,
                    "",
                    "",
                );
                push_dashboard_alert(
                    &state,
                    "error",
                    "upload",
                    "upload_processing_failed",
                    "Upload processing failed",
                    &failure_message,
                );
                record_admin_audit(
                    actor.clone(),
                    "upload",
                    "failed",
                    &source_ip,
                    "/api/upload",
                    format!("{} · {}", original_file_name, failure_message),
                );
            }
        }

        if temp_path.exists() {
            fs::remove_file(temp_path).ok();
        }
    }

    state.upload_status.lock().unwrap().remove(&upload_id);
    if !changed_urls.is_empty() {
        changed_urls.push(indexnow_home_url());
        spawn_indexnow_submission(state.clone(), changed_urls);
    } else if !processed_anything {
        upsert_upload_job(
            &state,
            &upload_id,
            "unknown",
            "failed",
            "Upload finished without a valid output file",
            &started_at,
            "",
            "",
        );
    }
    (StatusCode::OK, "Uploaded").into_response()
}
