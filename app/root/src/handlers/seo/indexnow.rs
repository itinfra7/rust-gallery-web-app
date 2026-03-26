use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
};
use serde::Serialize;
use std::{collections::BTreeSet, env};
use tracing::{info, warn};
use uuid::Uuid;
use crate::state::AppState;
use crate::handlers::dashboard::push_dashboard_alert;
use crate::handlers::common::{build_image_page_path, build_tag_path, make_absolute_url};

const BASE_URL: &str = "https://<PUBLIC_DOMAIN>";
const SITE_HOST: &str = "<PUBLIC_DOMAIN>";
const DEFAULT_INDEXNOW_ENDPOINT: &str = "https://api.indexnow.org/indexnow";

#[derive(Serialize)]
struct IndexNowPayload {
    host: &'static str,
    key: String,
    #[serde(rename = "keyLocation")]
    key_location: String,
    #[serde(rename = "urlList")]
    url_list: Vec<String>,
}

fn indexnow_key() -> Option<String> {
    env::var("INDEXNOW_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn indexnow_key_location_from_key(key: &str) -> String {
    format!("{}/{}.txt", BASE_URL, key)
}

fn indexnow_endpoint() -> String {
    env::var("INDEXNOW_ENDPOINT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_INDEXNOW_ENDPOINT.to_string())
}

fn summarize_urls(urls: &[String]) -> String {
    let mut summary = urls.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    if urls.len() > 3 {
        summary.push_str(", ...");
    }
    summary
}

pub fn indexnow_route_path() -> Option<String> {
    indexnow_key().map(|key| format!("/{}.txt", key))
}

pub fn indexnow_home_url() -> String {
    make_absolute_url("/")
}

pub fn indexnow_image_url(id: Uuid, tags: &[String], upload_date: &str) -> String {
    make_absolute_url(&build_image_page_path(&id.to_string(), tags, upload_date))
}

pub fn indexnow_tag_url(tag: &str) -> String {
    make_absolute_url(&build_tag_path(tag))
}

pub async fn indexnow_key_handler() -> impl IntoResponse {
    match indexnow_key() {
        Some(key) => (
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            key,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

pub fn spawn_indexnow_submission(state: AppState, urls: Vec<String>) {
    let Some(key) = indexnow_key() else {
        return;
    };

    let mut seen = BTreeSet::new();
    let url_list = urls
        .into_iter()
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .filter(|url| seen.insert(url.clone()))
        .collect::<Vec<_>>();

    if url_list.is_empty() {
        return;
    }

    let endpoint = indexnow_endpoint();
    let key_location = indexnow_key_location_from_key(&key);

    tokio::spawn(async move {
        let payload = IndexNowPayload {
            host: SITE_HOST,
            key,
            key_location,
            url_list: url_list.clone(),
        };

        let client = reqwest::Client::new();

        match client.post(&endpoint).json(&payload).send().await {
            Ok(response) if response.status().is_success() => {
                info!(
                    count = url_list.len(),
                    sample = %summarize_urls(&url_list),
                    "Submitted IndexNow URLs"
                );
            }
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                push_dashboard_alert(
                    &state,
                    "warn",
                    "indexnow",
                    "indexnow_non_success_status",
                    "IndexNow returned a non-success status",
                    format!(
                        "status={} count={} sample={} body={}",
                        status,
                        url_list.len(),
                        summarize_urls(&url_list),
                        body
                    ),
                );
                warn!(
                    status = %status,
                    body = %body,
                    count = url_list.len(),
                    sample = %summarize_urls(&url_list),
                    "IndexNow submission returned a non-success status"
                );
            }
            Err(error) => {
                push_dashboard_alert(
                    &state,
                    "error",
                    "indexnow",
                    "indexnow_submission_failed",
                    "IndexNow submission failed",
                    format!(
                        "error={} count={} sample={}",
                        error,
                        url_list.len(),
                        summarize_urls(&url_list)
                    ),
                );
                warn!(
                    error = %error,
                    count = url_list.len(),
                    sample = %summarize_urls(&url_list),
                    "Failed to submit IndexNow URLs"
                );
            }
        }
    });
}
