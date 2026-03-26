use askama::Template;
use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json, Response,
    },
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sqlx::Row;
use std::{
    collections::{BTreeMap, HashSet},
    convert::Infallible,
    ffi::CString,
    fs,
    mem::MaybeUninit,
    time::{Duration, Instant as StdInstant},
};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::alerting::spawn_runtime_alert;
use crate::auth::check_session;
use crate::audit::{load_recent_admin_audit, AdminAuditEntry};
use crate::state::{
    AppState, DashboardAlert, DashboardCrawlerHit, DashboardLiveEvent, DashboardUploadJob,
    OPS_STATUS_DIR,
};
use super::common::{build_image_page_path, build_tag_path, build_thumb_path, summarize_tags};

const DASHBOARD_REFRESH_INTERVAL_SECS: u64 = 10;

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {}

#[derive(Serialize)]
pub struct DiskStats {
    pub total: String,
    pub free: String,
    pub uploads: String,
    pub others: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub uploads_bytes: u64,
    pub others_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: f64,
}

#[derive(Serialize)]
pub struct DashboardOverview {
    pub total_images: i64,
    pub uploads_today: i64,
    pub comments_today: i64,
    pub likes_today: i64,
    pub unique_tags: i64,
    pub active_users_24h: i64,
    pub active_sessions: i64,
    pub pending_uploads: i64,
}

#[derive(Serialize)]
pub struct DashboardSystem {
    pub app_uptime_seconds: u64,
    pub db_latency_ms: u64,
    pub total_comments: i64,
    pub total_likes: i64,
    pub latest_upload_at: Option<String>,
    pub latest_comment_at: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct OpsTableCounts {
    pub users: i64,
    pub images: i64,
    pub likes: i64,
    pub comments: i64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct BackupDbSummary {
    pub format_version: Option<u32>,
    pub created_at: Option<String>,
    pub database_name: Option<String>,
    pub table_counts: Option<OpsTableCounts>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct BackupStatus {
    pub status: String,
    pub recorded_at: String,
    pub snapshot_id: String,
    pub snapshot_dir: String,
    pub uploads_original_count: i64,
    pub uploads_thumb_count: i64,
    pub db_summary: Option<BackupDbSummary>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct RehearsalDbSummary {
    pub format_version: Option<u32>,
    pub created_at: Option<String>,
    pub database_name: Option<String>,
    pub table_counts: Option<OpsTableCounts>,
    pub rehearsal_counts: Option<OpsTableCounts>,
    pub likes_missing_users: Option<i64>,
    pub likes_missing_images: Option<i64>,
    pub comments_missing_users: Option<i64>,
    pub comments_missing_images: Option<i64>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct RehearsalStatus {
    pub status: String,
    pub recorded_at: String,
    pub snapshot_dir: String,
    pub rehearsal_dir: String,
    pub uploads_original_count: i64,
    pub uploads_thumb_count: i64,
    pub manifest_upload_count: Option<i64>,
    pub manifest_thumb_count: Option<i64>,
    pub db_rehearsal: Option<RehearsalDbSummary>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct RebuildRecord {
    pub status: String,
    pub recorded_at: String,
    pub duration_seconds: i64,
    pub service_action: String,
    pub detail: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct AlertDeliveryStatus {
    #[serde(default)]
    pub recorded_at: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub dedupe_key: Option<String>,
    #[serde(default)]
    pub delivery_status: Option<String>,
    #[serde(default)]
    pub failure_streak: Option<i64>,
    #[serde(default)]
    pub failure_threshold: Option<i64>,
    #[serde(default)]
    pub suppressed_duplicates: Option<i64>,
}

#[derive(Clone, Serialize)]
pub struct AlertDeliverySummary {
    pub attempts: i64,
    pub sent: i64,
    pub failed: i64,
    pub suppressed: i64,
    pub recorded_only: i64,
    pub skipped_config: i64,
    pub last_sent_at: Option<String>,
    pub last_failed_at: Option<String>,
    pub last_delivery_at: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct MaintenanceStatus {
    pub status: String,
    pub recorded_at: String,
    pub alerts_lines: i64,
    pub admin_audit_lines: i64,
    pub rebuild_history_lines: i64,
    pub removed_rehearsal_dirs: i64,
    pub removed_pre_restore_dirs: i64,
    pub removed_dedupe_files: i64,
    #[serde(default)]
    pub crawler_hit_lines: Option<i64>,
    #[serde(default)]
    pub smoke_history_lines: Option<i64>,
    #[serde(default)]
    pub rollback_history_lines: Option<i64>,
    #[serde(default)]
    pub disk_free_bytes: Option<i64>,
    #[serde(default)]
    pub disk_free_gb: Option<i64>,
    #[serde(default)]
    pub disk_threshold_gb: Option<i64>,
    #[serde(default)]
    pub disk_status: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SmokeCheckStatus {
    pub status: String,
    pub recorded_at: String,
    pub base_url: String,
    pub checked_paths: Vec<String>,
    #[serde(default)]
    pub representative_path: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct RollbackStatus {
    pub status: String,
    pub recorded_at: String,
    pub duration_seconds: i64,
    pub service_action: String,
    pub target_binary: String,
    pub detail: String,
}

#[derive(Clone, Serialize)]
pub struct CrawlerIssueSummary {
    pub tracked_hits: i64,
    pub not_found_404: i64,
    pub server_errors_5xx: i64,
    pub last_404_at: Option<String>,
    pub last_5xx_at: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct DashboardOperations {
    pub latest_backup: Option<BackupStatus>,
    pub latest_rehearsal: Option<RehearsalStatus>,
    pub latest_maintenance: Option<MaintenanceStatus>,
    pub latest_smoke_check: Option<SmokeCheckStatus>,
    pub latest_rollback: Option<RollbackStatus>,
    pub rebuild_history: Vec<RebuildRecord>,
    pub latest_alert_delivery: Option<AlertDeliveryStatus>,
    pub alert_delivery_summary: AlertDeliverySummary,
    pub crawler_issues: CrawlerIssueSummary,
}

#[derive(Serialize)]
pub struct TrendPoint {
    pub label: String,
    pub uploads: i64,
    pub comments: i64,
    pub likes: i64,
}

#[derive(Serialize)]
pub struct TagStat {
    pub name: String,
    pub url: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct RecentUploadCard {
    pub id: String,
    pub page_url: String,
    pub thumb_url: String,
    pub title: String,
    pub tag_line: String,
    pub uploaded_at: String,
    pub like_count: i32,
    pub comment_count: i64,
}

#[derive(Serialize)]
pub struct CrawlerSummary {
    pub crawler: String,
    pub hits: i64,
    pub ok_200: i64,
    pub not_modified_304: i64,
    pub not_found_404: i64,
    pub other_statuses: i64,
    pub last_seen: Option<String>,
}

#[derive(Serialize)]
pub struct CrawlerSurfaceSummary {
    pub surface: String,
    pub hits: i64,
    pub ok_200: i64,
    pub not_modified_304: i64,
    pub not_found_404: i64,
    pub server_errors_5xx: i64,
    pub unique_paths: i64,
    pub last_seen: Option<String>,
}

#[derive(Serialize)]
pub struct CrawlerPathSummary {
    pub path: String,
    pub hits: i64,
    pub last_status: u16,
    pub last_seen: Option<String>,
}

#[derive(Serialize)]
pub struct DashboardSnapshot {
    pub generated_at: String,
    pub refresh_interval_secs: u64,
    pub overview: DashboardOverview,
    pub system: DashboardSystem,
    pub operations: DashboardOperations,
    pub disk: DiskStats,
    pub trends: Vec<TrendPoint>,
    pub top_tags: Vec<TagStat>,
    pub recent_uploads: Vec<RecentUploadCard>,
    pub admin_audit: Vec<AdminAuditEntry>,
    pub activities: Vec<DashboardLiveEvent>,
    pub alerts: Vec<DashboardAlert>,
    pub upload_jobs: Vec<DashboardUploadJob>,
    pub crawler_summaries: Vec<CrawlerSummary>,
    pub crawler_surface_summaries: Vec<CrawlerSurfaceSummary>,
    pub crawler_top_paths: Vec<CrawlerPathSummary>,
    pub recent_crawler_hits: Vec<DashboardCrawlerHit>,
}

impl AlertDeliveryStatus {
    fn event_timestamp(&self) -> Option<&str> {
        self.recorded_at
            .as_deref()
            .or(self.timestamp.as_deref())
    }
}

struct ActivityDraft {
    kind: &'static str,
    label: String,
    detail: String,
    timestamp: DateTime<Utc>,
    page_url: String,
    thumb_url: String,
}

struct CrawlerSummaryAccumulator {
    hits: i64,
    ok_200: i64,
    not_modified_304: i64,
    not_found_404: i64,
    other_statuses: i64,
    last_seen: Option<String>,
}

struct CrawlerSurfaceAccumulator {
    hits: i64,
    ok_200: i64,
    not_modified_304: i64,
    not_found_404: i64,
    server_errors_5xx: i64,
    unique_paths: HashSet<String>,
    last_seen: Option<String>,
}

struct CrawlerPathAccumulator {
    hits: i64,
    last_status: u16,
    last_seen: Option<String>,
}

pub fn publish_dashboard_event(
    state: &AppState,
    kind: &str,
    label: &str,
    detail: impl Into<String>,
    page_url: impl Into<String>,
    thumb_url: impl Into<String>,
) {
    state.publish_live_event(DashboardLiveEvent {
        kind: kind.to_string(),
        label: label.to_string(),
        detail: detail.into(),
        timestamp: Utc::now().to_rfc3339(),
        page_url: page_url.into(),
        thumb_url: thumb_url.into(),
    });
}

pub fn push_dashboard_alert(
    state: &AppState,
    severity: &str,
    source: &str,
    kind: &str,
    label: &str,
    detail: impl Into<String>,
) {
    let detail = detail.into();
    state.push_alert(DashboardAlert {
        severity: severity.to_string(),
        source: source.to_string(),
        label: label.to_string(),
        detail: detail.clone(),
        timestamp: Utc::now().to_rfc3339(),
    });
    spawn_runtime_alert(severity, source, kind, label, detail);
}

pub async fn publish_image_activity_event(
    state: &AppState,
    image_id: Uuid,
    kind: &str,
    label: &str,
    detail_override: Option<String>,
) {
    let row = match sqlx::query("SELECT tags, upload_date FROM images WHERE id = $1")
        .bind(image_id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(row)) => row,
        _ => return,
    };

    let tags: Vec<String> = row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default();
    let upload_date: DateTime<Utc> = row.get("upload_date");
    let (page_url, thumb_url, title, tag_line) = build_image_paths(image_id, &tags, upload_date);
    let detail = detail_override
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| format!("{} · {}", title, value))
        .unwrap_or_else(|| format!("{} · {}", title, tag_line));

    publish_dashboard_event(state, kind, label, detail, page_url, thumb_url);
}

pub async fn crawler_telemetry_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let user_agent = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let path = request.uri().path().to_string();
    let method = request.method().to_string();
    let crawler = detect_search_crawler(&user_agent)
        .filter(|_| should_track_crawler_path(&path))
        .map(str::to_string);

    let response = next.run(request).await;

    if let Some(crawler) = crawler {
        state.record_crawler_hit(DashboardCrawlerHit {
            crawler,
            method,
            path,
            status: response.status().as_u16(),
            timestamp: Utc::now().to_rfc3339(),
        });
    }

    response
}

fn detect_search_crawler(user_agent: &str) -> Option<&'static str> {
    if user_agent.contains("googlebot")
        || user_agent.contains("google-inspectiontool")
        || user_agent.contains("googleother")
    {
        Some("Google")
    } else if user_agent.contains("bingbot") || user_agent.contains("adidxbot") {
        Some("Bing")
    } else if user_agent.contains("yandex") {
        Some("Yandex")
    } else if user_agent.contains("yeti") || user_agent.contains("naver") {
        Some("Naver")
    } else {
        None
    }
}

fn should_track_crawler_path(path: &str) -> bool {
    path == "/"
        || path == "/robots.txt"
        || path == "/sitemap.xml"
        || path == "/rss.xml"
        || path.starts_with("/image/")
        || path.starts_with("/tag/")
        || path.starts_with("/media/")
        || path.starts_with("/thumbs/")
        || path.starts_with("/uploads/")
}

fn format_size(bytes: u64) -> String {
    const UNIT: u64 = 1024;
    if bytes < UNIT {
        return format!("{} B", bytes);
    }
    let exp = (bytes as f64).ln() / (UNIT as f64).ln();
    let pre = "KMGTPE".chars().nth(exp as usize - 1).unwrap_or('?');
    format!(
        "{:.2} {}B",
        (bytes as f64) / (UNIT as f64).powi(exp as i32),
        pre
    )
}

fn get_dir_size(path: &str) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

fn get_fs_stats(path: &str) -> (u64, u64) {
    let path_c = match CString::new(path) {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };

    unsafe {
        let mut stats: libc::statvfs = MaybeUninit::zeroed().assume_init();
        if libc::statvfs(path_c.as_ptr(), &mut stats) == 0 {
            let total = stats.f_blocks as u64 * stats.f_frsize as u64;
            let free = stats.f_bavail as u64 * stats.f_frsize as u64;
            (total, free)
        } else {
            (0, 0)
        }
    }
}

fn collect_disk_stats() -> DiskStats {
    let (total_space, available_space) = get_fs_stats("/");
    let uploads_size = get_dir_size("uploads");
    let used_space = total_space.saturating_sub(available_space);
    let others_size = used_space.saturating_sub(uploads_size);
    let used_percent = if total_space == 0 {
        0.0
    } else {
        (used_space as f64 / total_space as f64) * 100.0
    };

    DiskStats {
        total: format_size(total_space),
        free: format_size(available_space),
        uploads: format_size(uploads_size),
        others: format_size(others_size),
        total_bytes: total_space,
        free_bytes: available_space,
        uploads_bytes: uploads_size,
        others_bytes: others_size,
        used_bytes: used_space,
        used_percent,
    }
}

fn build_image_label(tags: &[String], image_id: Uuid) -> String {
    let summary = summarize_tags(tags, 4);
    if summary.is_empty() {
        format!("Image {}", &image_id.to_string()[..8])
    } else {
        summary
    }
}

pub fn build_image_paths(
    image_id: Uuid,
    tags: &[String],
    upload_date: DateTime<Utc>,
) -> (String, String, String, String) {
    let upload_date_str = upload_date.format("%Y-%m-%d").to_string();
    let image_id_str = image_id.to_string();
    let page_url = build_image_page_path(&image_id_str, tags, &upload_date_str);
    let thumb_url = build_thumb_path(&image_id_str, tags, &upload_date_str);
    let title = build_image_label(tags, image_id);
    let tag_line = if tags.is_empty() {
        "No tags".to_string()
    } else {
        summarize_tags(tags, 8)
    };

    (page_url, thumb_url, title, tag_line)
}

fn sort_and_dedupe_activities(mut items: Vec<DashboardLiveEvent>) -> Vec<DashboardLiveEvent> {
    items.sort_by(|left, right| {
        parse_activity_timestamp(&right.timestamp).cmp(&parse_activity_timestamp(&left.timestamp))
    });

    let mut seen = HashSet::new();
    items.retain(|item| {
        seen.insert(format!(
            "{}|{}|{}|{}",
            item.kind, item.label, item.detail, item.page_url
        ))
    });

    items.truncate(18);
    items
}

fn parse_activity_timestamp(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now() - ChronoDuration::days(3650))
}

fn build_crawler_summaries(hits: &[DashboardCrawlerHit]) -> Vec<CrawlerSummary> {
    let mut summaries = BTreeMap::<String, CrawlerSummaryAccumulator>::new();

    for hit in hits {
        let entry = summaries
            .entry(hit.crawler.clone())
            .or_insert(CrawlerSummaryAccumulator {
                hits: 0,
                ok_200: 0,
                not_modified_304: 0,
                not_found_404: 0,
                other_statuses: 0,
                last_seen: None,
            });

        entry.hits += 1;
        match hit.status {
            200 => entry.ok_200 += 1,
            304 => entry.not_modified_304 += 1,
            404 => entry.not_found_404 += 1,
            _ => entry.other_statuses += 1,
        }

        if entry
            .last_seen
            .as_ref()
            .map(|existing| existing < &hit.timestamp)
            .unwrap_or(true)
        {
            entry.last_seen = Some(hit.timestamp.clone());
        }
    }

    let mut values = summaries
        .into_iter()
        .map(|(crawler, summary)| CrawlerSummary {
            crawler,
            hits: summary.hits,
            ok_200: summary.ok_200,
            not_modified_304: summary.not_modified_304,
            not_found_404: summary.not_found_404,
            other_statuses: summary.other_statuses,
            last_seen: summary.last_seen,
        })
        .collect::<Vec<_>>();

    values.sort_by(|left, right| right.hits.cmp(&left.hits).then_with(|| left.crawler.cmp(&right.crawler)));
    values.truncate(6);
    values
}

fn build_crawler_issue_summary(hits: &[DashboardCrawlerHit]) -> CrawlerIssueSummary {
    let not_found_404 = hits.iter().filter(|hit| hit.status == 404).count() as i64;
    let server_errors_5xx = hits
        .iter()
        .filter(|hit| (500..=599).contains(&hit.status))
        .count() as i64;
    let last_404_at = hits
        .iter()
        .find(|hit| hit.status == 404)
        .map(|hit| hit.timestamp.clone());
    let last_5xx_at = hits
        .iter()
        .find(|hit| (500..=599).contains(&hit.status))
        .map(|hit| hit.timestamp.clone());

    CrawlerIssueSummary {
        tracked_hits: hits.len() as i64,
        not_found_404,
        server_errors_5xx,
        last_404_at,
        last_5xx_at,
    }
}

fn classify_crawler_surface(path: &str) -> &'static str {
    if path == "/" {
        "home"
    } else if path == "/robots.txt" {
        "robots"
    } else if path == "/sitemap.xml" {
        "sitemap"
    } else if path == "/rss.xml" {
        "rss"
    } else if path.starts_with("/image/") {
        "image-page"
    } else if path.starts_with("/tag/") {
        "tag-page"
    } else if path.starts_with("/media/") {
        "media"
    } else if path.starts_with("/thumbs/") {
        "thumb"
    } else if path.starts_with("/uploads/") {
        "uploads-legacy"
    } else {
        "other"
    }
}

fn build_crawler_surface_summaries(hits: &[DashboardCrawlerHit]) -> Vec<CrawlerSurfaceSummary> {
    let mut summaries = BTreeMap::<String, CrawlerSurfaceAccumulator>::new();

    for hit in hits {
        let surface = classify_crawler_surface(&hit.path).to_string();
        let entry = summaries
            .entry(surface)
            .or_insert(CrawlerSurfaceAccumulator {
                hits: 0,
                ok_200: 0,
                not_modified_304: 0,
                not_found_404: 0,
                server_errors_5xx: 0,
                unique_paths: HashSet::new(),
                last_seen: None,
            });

        entry.hits += 1;
        entry.unique_paths.insert(hit.path.clone());
        match hit.status {
            200 => entry.ok_200 += 1,
            304 => entry.not_modified_304 += 1,
            404 => entry.not_found_404 += 1,
            500..=599 => entry.server_errors_5xx += 1,
            _ => {}
        }

        if entry
            .last_seen
            .as_ref()
            .map(|existing| existing < &hit.timestamp)
            .unwrap_or(true)
        {
            entry.last_seen = Some(hit.timestamp.clone());
        }
    }

    let mut values = summaries
        .into_iter()
        .map(|(surface, summary)| CrawlerSurfaceSummary {
            surface,
            hits: summary.hits,
            ok_200: summary.ok_200,
            not_modified_304: summary.not_modified_304,
            not_found_404: summary.not_found_404,
            server_errors_5xx: summary.server_errors_5xx,
            unique_paths: summary.unique_paths.len() as i64,
            last_seen: summary.last_seen,
        })
        .collect::<Vec<_>>();

    values.sort_by(|left, right| {
        right
            .hits
            .cmp(&left.hits)
            .then_with(|| left.surface.cmp(&right.surface))
    });
    values.truncate(8);
    values
}

fn build_crawler_top_paths(hits: &[DashboardCrawlerHit]) -> Vec<CrawlerPathSummary> {
    let mut summaries = BTreeMap::<String, CrawlerPathAccumulator>::new();

    for hit in hits {
        let entry = summaries
            .entry(hit.path.clone())
            .or_insert(CrawlerPathAccumulator {
                hits: 0,
                last_status: hit.status,
                last_seen: None,
            });

        entry.hits += 1;
        if entry
            .last_seen
            .as_ref()
            .map(|existing| existing <= &hit.timestamp)
            .unwrap_or(true)
        {
            entry.last_seen = Some(hit.timestamp.clone());
            entry.last_status = hit.status;
        }
    }

    let mut values = summaries
        .into_iter()
        .map(|(path, summary)| CrawlerPathSummary {
            path,
            hits: summary.hits,
            last_status: summary.last_status,
            last_seen: summary.last_seen,
        })
        .collect::<Vec<_>>();

    values.sort_by(|left, right| {
        right
            .hits
            .cmp(&left.hits)
            .then_with(|| left.path.cmp(&right.path))
    });
    values.truncate(8);
    values
}

fn build_alert_delivery_summary(records: &[AlertDeliveryStatus]) -> AlertDeliverySummary {
    let mut summary = AlertDeliverySummary {
        attempts: records.len() as i64,
        sent: 0,
        failed: 0,
        suppressed: 0,
        recorded_only: 0,
        skipped_config: 0,
        last_sent_at: None,
        last_failed_at: None,
        last_delivery_at: None,
    };

    for record in records {
        let status = record.delivery_status.as_deref().unwrap_or("unknown");
        let timestamp = record.event_timestamp().map(str::to_string);

        match status {
            "sent" => {
                summary.sent += 1;
                if summary
                    .last_sent_at
                    .as_ref()
                    .map(|existing| timestamp.as_ref().map(|value| existing < value).unwrap_or(false))
                    .unwrap_or(timestamp.is_some())
                {
                    summary.last_sent_at = timestamp.clone();
                }
            }
            "failed" => {
                summary.failed += 1;
                if summary
                    .last_failed_at
                    .as_ref()
                    .map(|existing| timestamp.as_ref().map(|value| existing < value).unwrap_or(false))
                    .unwrap_or(timestamp.is_some())
                {
                    summary.last_failed_at = timestamp.clone();
                }
            }
            "suppressed" => summary.suppressed += 1,
            "recorded-only" => summary.recorded_only += 1,
            "skipped-config" => summary.skipped_config += 1,
            _ => {}
        }

        if summary
            .last_delivery_at
            .as_ref()
            .map(|existing| timestamp.as_ref().map(|value| existing < value).unwrap_or(false))
            .unwrap_or(timestamp.is_some())
        {
            summary.last_delivery_at = timestamp;
        }
    }

    summary
}

fn read_ops_json<T: DeserializeOwned>(path: &str) -> Option<T> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn read_ops_ndjson<T: DeserializeOwned>(path: &str, limit: usize) -> Vec<T> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut values = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<T>(line).ok())
        .collect::<Vec<_>>();

    values.reverse();
    values.truncate(limit);
    values
}

async fn collect_dashboard_snapshot(state: &AppState) -> Result<DashboardSnapshot, sqlx::Error> {
    let now = Utc::now();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let active_user_threshold = now - ChronoDuration::hours(24);
    let trend_start = now.date_naive() - ChronoDuration::days(6);

    let overview_row = sqlx::query(
        r#"
        SELECT
            (SELECT COUNT(*) FROM images) AS total_images,
            (SELECT COUNT(*) FROM images WHERE upload_date >= $1) AS uploads_today,
            (SELECT COUNT(*) FROM comments WHERE created_at >= $1) AS comments_today,
            (SELECT COUNT(*) FROM likes WHERE created_at >= $1) AS likes_today,
            (SELECT COUNT(*) FROM likes) AS total_likes,
            (SELECT COUNT(*) FROM comments) AS total_comments,
            (SELECT COUNT(*) FROM users WHERE last_activity_at >= $2) AS active_users_24h,
            (
                SELECT COUNT(*)
                FROM (
                    SELECT DISTINCT unnest(tags) AS tag
                    FROM images
                    WHERE tags IS NOT NULL
                ) tag_pool
            ) AS unique_tags
        "#
    )
    .bind(today_start)
    .bind(active_user_threshold)
    .fetch_one(&state.db)
    .await?;

    let recent_upload_jobs = state.recent_upload_jobs();
    let pending_uploads = recent_upload_jobs
        .iter()
        .filter(|job| job.state == "running")
        .count() as i64;

    let active_sessions = state.active_session_count();

    let db_ping_started = StdInstant::now();
    let _ = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await?;
    let db_latency_ms = db_ping_started.elapsed().as_millis() as u64;

    let latest_upload_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        "SELECT upload_date FROM images ORDER BY upload_date DESC LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .map(|value| value.to_rfc3339());

    let latest_comment_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        "SELECT created_at FROM comments ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .map(|value| value.to_rfc3339());

    let trend_rows = sqlx::query(
        r#"
        WITH days AS (
            SELECT generate_series($1::date, $2::date, interval '1 day')::date AS day
        ),
        uploads AS (
            SELECT DATE(upload_date AT TIME ZONE 'UTC') AS day, COUNT(*)::bigint AS count
            FROM images
            WHERE upload_date >= $3
            GROUP BY 1
        ),
        comments AS (
            SELECT DATE(created_at AT TIME ZONE 'UTC') AS day, COUNT(*)::bigint AS count
            FROM comments
            WHERE created_at >= $3
            GROUP BY 1
        ),
        likes AS (
            SELECT DATE(created_at AT TIME ZONE 'UTC') AS day, COUNT(*)::bigint AS count
            FROM likes
            WHERE created_at >= $3
            GROUP BY 1
        )
        SELECT
            TO_CHAR(days.day, 'MM/DD') AS label,
            COALESCE(uploads.count, 0) AS uploads,
            COALESCE(comments.count, 0) AS comments,
            COALESCE(likes.count, 0) AS likes
        FROM days
        LEFT JOIN uploads ON uploads.day = days.day
        LEFT JOIN comments ON comments.day = days.day
        LEFT JOIN likes ON likes.day = days.day
        ORDER BY days.day ASC
        "#,
    )
    .bind(trend_start)
    .bind(now.date_naive())
    .bind(today_start - ChronoDuration::days(6))
    .fetch_all(&state.db)
    .await?;

    let trends = trend_rows
        .into_iter()
        .map(|row| TrendPoint {
            label: row.get("label"),
            uploads: row.get("uploads"),
            comments: row.get("comments"),
            likes: row.get("likes"),
        })
        .collect::<Vec<_>>();

    let top_tag_rows = sqlx::query(
        r#"
        SELECT tag, COUNT(*)::bigint AS count
        FROM images
        CROSS JOIN LATERAL unnest(tags) AS tag
        GROUP BY tag
        ORDER BY COUNT(*) DESC, MAX(upload_date) DESC, tag
        LIMIT 10
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let top_tags = top_tag_rows
        .into_iter()
        .map(|row| {
            let tag_name: String = row.get("tag");
            TagStat {
                url: build_tag_path(&tag_name),
                name: tag_name,
                count: row.get("count"),
            }
        })
        .collect::<Vec<_>>();

    let recent_upload_rows = sqlx::query(
        r#"
        SELECT
            i.id, i.filename, i.tags, i.upload_date, i.like_count,
            (SELECT COUNT(*) FROM comments c WHERE c.image_id = i.id) AS comment_count
        FROM images i
        ORDER BY i.upload_date DESC
        LIMIT 8
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let recent_uploads = recent_upload_rows
        .iter()
        .map(|row| {
            let image_id: Uuid = row.get("id");
            let tags: Vec<String> = row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default();
            let upload_date: DateTime<Utc> = row.get("upload_date");
            let (page_url, thumb_url, title, tag_line) = build_image_paths(image_id, &tags, upload_date);

            RecentUploadCard {
                id: image_id.to_string(),
                page_url,
                thumb_url,
                title,
                tag_line,
                uploaded_at: upload_date.to_rfc3339(),
                like_count: row.get("like_count"),
                comment_count: row.get("comment_count"),
            }
        })
        .collect::<Vec<_>>();

    let recent_comment_rows = sqlx::query(
        r#"
        SELECT
            c.content, c.created_at, i.id AS image_id, i.tags, i.upload_date
        FROM comments c
        JOIN images i ON i.id = c.image_id
        ORDER BY c.created_at DESC
        LIMIT 12
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let recent_like_rows = sqlx::query(
        r#"
        SELECT
            l.created_at, i.id AS image_id, i.tags, i.upload_date
        FROM likes l
        JOIN images i ON i.id = l.image_id
        ORDER BY l.created_at DESC
        LIMIT 12
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut db_activities = recent_upload_rows
        .into_iter()
        .map(|row| {
            let image_id: Uuid = row.get("id");
            let tags: Vec<String> = row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default();
            let upload_date: DateTime<Utc> = row.get("upload_date");
            let (page_url, thumb_url, title, tag_line) = build_image_paths(image_id, &tags, upload_date);
            ActivityDraft {
                kind: "upload",
                label: "New upload".to_string(),
                detail: format!("{} · {}", title, tag_line),
                timestamp: upload_date,
                page_url,
                thumb_url,
            }
        })
        .collect::<Vec<_>>();

    db_activities.extend(recent_comment_rows.into_iter().map(|row| {
        let image_id: Uuid = row.get("image_id");
        let tags: Vec<String> = row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default();
        let upload_date: DateTime<Utc> = row.get("upload_date");
        let created_at: DateTime<Utc> = row.get("created_at");
        let content: String = row.get("content");
        let (page_url, thumb_url, title, _) = build_image_paths(image_id, &tags, upload_date);
        ActivityDraft {
            kind: "comment",
            label: "New comment".to_string(),
            detail: format!("{} · {}", title, content),
            timestamp: created_at,
            page_url,
            thumb_url,
        }
    }));

    db_activities.extend(recent_like_rows.into_iter().map(|row| {
        let image_id: Uuid = row.get("image_id");
        let tags: Vec<String> = row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default();
        let upload_date: DateTime<Utc> = row.get("upload_date");
        let created_at: DateTime<Utc> = row.get("created_at");
        let (page_url, thumb_url, title, tag_line) = build_image_paths(image_id, &tags, upload_date);
        ActivityDraft {
            kind: "like",
            label: "New like".to_string(),
            detail: format!("{} · {}", title, tag_line),
            timestamp: created_at,
            page_url,
            thumb_url,
        }
    }));

    let mut activities = db_activities
        .into_iter()
        .map(|item| DashboardLiveEvent {
            kind: item.kind.to_string(),
            label: item.label,
            detail: item.detail,
            timestamp: item.timestamp.to_rfc3339(),
            page_url: item.page_url,
            thumb_url: item.thumb_url,
        })
        .collect::<Vec<_>>();
    activities.extend(state.recent_live_events());
    let activities = sort_and_dedupe_activities(activities);

    let crawler_hits = state.recent_crawler_hits();
    let crawler_summaries = build_crawler_summaries(&crawler_hits);
    let crawler_surface_summaries = build_crawler_surface_summaries(&crawler_hits);
    let crawler_top_paths = build_crawler_top_paths(&crawler_hits);
    let crawler_issues = build_crawler_issue_summary(&crawler_hits);
    let recent_crawler_hits = crawler_hits.into_iter().take(12).collect::<Vec<_>>();

    let alerts = state.recent_alerts().into_iter().take(12).collect::<Vec<_>>();
    let upload_jobs = recent_upload_jobs.into_iter().take(10).collect::<Vec<_>>();
    let disk = collect_disk_stats();
    let recent_alert_deliveries = read_ops_ndjson::<AlertDeliveryStatus>("run/alerts.ndjson", 128);
    let operations = DashboardOperations {
        latest_backup: read_ops_json(&format!("{}/latest-backup.json", OPS_STATUS_DIR)),
        latest_rehearsal: read_ops_json(&format!("{}/latest-restore-rehearsal.json", OPS_STATUS_DIR)),
        latest_maintenance: read_ops_json(&format!("{}/latest-maintenance.json", OPS_STATUS_DIR)),
        latest_smoke_check: read_ops_json(&format!("{}/latest-smoke-check.json", OPS_STATUS_DIR)),
        latest_rollback: read_ops_json(&format!("{}/latest-rollback.json", OPS_STATUS_DIR)),
        rebuild_history: read_ops_ndjson(&format!("{}/rebuild-history.ndjson", OPS_STATUS_DIR), 8),
        latest_alert_delivery: read_ops_json(&format!("{}/latest-alert-delivery.json", OPS_STATUS_DIR))
            .or_else(|| recent_alert_deliveries.first().cloned()),
        alert_delivery_summary: build_alert_delivery_summary(&recent_alert_deliveries),
        crawler_issues,
    };
    let admin_audit = load_recent_admin_audit(12);

    Ok(DashboardSnapshot {
        generated_at: now.to_rfc3339(),
        refresh_interval_secs: DASHBOARD_REFRESH_INTERVAL_SECS,
        overview: DashboardOverview {
            total_images: overview_row.get("total_images"),
            uploads_today: overview_row.get("uploads_today"),
            comments_today: overview_row.get("comments_today"),
            likes_today: overview_row.get("likes_today"),
            unique_tags: overview_row.get("unique_tags"),
            active_users_24h: overview_row.get("active_users_24h"),
            active_sessions,
            pending_uploads,
        },
        system: DashboardSystem {
            app_uptime_seconds: state.app_started_at.elapsed().as_secs(),
            db_latency_ms,
            total_comments: overview_row.get("total_comments"),
            total_likes: overview_row.get("total_likes"),
            latest_upload_at,
            latest_comment_at,
        },
        operations,
        disk,
        trends,
        top_tags,
        recent_uploads,
        admin_audit,
        activities,
        alerts,
        upload_jobs,
        crawler_summaries,
        crawler_surface_summaries,
        crawler_top_paths,
        recent_crawler_hits,
    })
}

pub async fn dashboard_page_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    if !check_session(&state, &jar) {
        return axum::response::Redirect::to("/").into_response();
    }

    DashboardTemplate {}.into_response()
}

pub async fn dashboard_activity_stream_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    if !check_session(&state, &jar) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let stream = BroadcastStream::new(state.subscribe_activity_stream()).filter_map(|message| {
        match message {
            Ok(payload) => Some(Ok::<Event, Infallible>(
                Event::default().event("activity").data(payload),
            )),
            Err(_) => None,
        }
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive"))
        .into_response()
}

pub async fn get_dashboard_overview_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    if !check_session(&state, &jar) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    match collect_dashboard_snapshot(&state).await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => {
            push_dashboard_alert(
                &state,
                "error",
                "dashboard",
                "dashboard_snapshot_failed",
                "Dashboard snapshot failed",
                format!("Failed to build dashboard snapshot: {}", error),
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to build dashboard",
            )
                .into_response()
        }
    }
}

pub async fn get_disk_stats_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    if !check_session(&state, &jar) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    Json(collect_disk_stats()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_alert_delivery_summary_tracks_statuses_and_last_timestamps() {
        let summary = build_alert_delivery_summary(&[
            AlertDeliveryStatus {
                recorded_at: Some("2026-03-21T10:00:00Z".to_string()),
                timestamp: None,
                host: None,
                source: Some("backup".to_string()),
                severity: Some("info".to_string()),
                kind: Some("backup_success".to_string()),
                summary: None,
                detail: None,
                dedupe_key: None,
                delivery_status: Some("recorded-only".to_string()),
                failure_streak: None,
                failure_threshold: None,
                suppressed_duplicates: None,
            },
            AlertDeliveryStatus {
                recorded_at: Some("2026-03-21T10:01:00Z".to_string()),
                timestamp: None,
                host: None,
                source: Some("runtime".to_string()),
                severity: Some("error".to_string()),
                kind: Some("runtime_error".to_string()),
                summary: None,
                detail: None,
                dedupe_key: None,
                delivery_status: Some("sent".to_string()),
                failure_streak: Some(1),
                failure_threshold: Some(1),
                suppressed_duplicates: Some(0),
            },
            AlertDeliveryStatus {
                recorded_at: Some("2026-03-21T10:02:00Z".to_string()),
                timestamp: None,
                host: None,
                source: Some("runtime".to_string()),
                severity: Some("error".to_string()),
                kind: Some("runtime_error".to_string()),
                summary: None,
                detail: None,
                dedupe_key: None,
                delivery_status: Some("suppressed".to_string()),
                failure_streak: Some(1),
                failure_threshold: Some(2),
                suppressed_duplicates: Some(1),
            },
            AlertDeliveryStatus {
                recorded_at: Some("2026-03-21T10:03:00Z".to_string()),
                timestamp: None,
                host: None,
                source: Some("runtime".to_string()),
                severity: Some("error".to_string()),
                kind: Some("runtime_error".to_string()),
                summary: None,
                detail: None,
                dedupe_key: None,
                delivery_status: Some("failed".to_string()),
                failure_streak: Some(3),
                failure_threshold: Some(1),
                suppressed_duplicates: Some(0),
            },
        ]);

        assert_eq!(summary.attempts, 4);
        assert_eq!(summary.sent, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.suppressed, 1);
        assert_eq!(summary.recorded_only, 1);
        assert_eq!(summary.last_sent_at.as_deref(), Some("2026-03-21T10:01:00Z"));
        assert_eq!(summary.last_failed_at.as_deref(), Some("2026-03-21T10:03:00Z"));
        assert_eq!(summary.last_delivery_at.as_deref(), Some("2026-03-21T10:03:00Z"));
    }

    #[test]
    fn crawler_surface_summaries_group_and_count_by_surface() {
        let summaries = build_crawler_surface_summaries(&[
            DashboardCrawlerHit {
                crawler: "Google".to_string(),
                method: "GET".to_string(),
                path: "/".to_string(),
                status: 200,
                timestamp: "2026-03-21T10:00:00Z".to_string(),
            },
            DashboardCrawlerHit {
                crawler: "Google".to_string(),
                method: "GET".to_string(),
                path: "/sitemap.xml".to_string(),
                status: 304,
                timestamp: "2026-03-21T10:01:00Z".to_string(),
            },
            DashboardCrawlerHit {
                crawler: "Bing".to_string(),
                method: "GET".to_string(),
                path: "/image/abc/slug".to_string(),
                status: 404,
                timestamp: "2026-03-21T10:02:00Z".to_string(),
            },
        ]);

        assert_eq!(summaries[0].surface, "home");
        assert_eq!(summaries[0].hits, 1);
        assert_eq!(summaries[1].surface, "image-page");
        assert_eq!(summaries[1].not_found_404, 1);
        assert_eq!(summaries[2].surface, "sitemap");
        assert_eq!(summaries[2].not_modified_304, 1);
    }

    #[test]
    fn crawler_top_paths_rank_by_hit_count() {
        let top_paths = build_crawler_top_paths(&[
            DashboardCrawlerHit {
                crawler: "Google".to_string(),
                method: "GET".to_string(),
                path: "/sitemap.xml".to_string(),
                status: 200,
                timestamp: "2026-03-21T10:00:00Z".to_string(),
            },
            DashboardCrawlerHit {
                crawler: "Google".to_string(),
                method: "GET".to_string(),
                path: "/".to_string(),
                status: 200,
                timestamp: "2026-03-21T10:01:00Z".to_string(),
            },
            DashboardCrawlerHit {
                crawler: "Bing".to_string(),
                method: "GET".to_string(),
                path: "/sitemap.xml".to_string(),
                status: 304,
                timestamp: "2026-03-21T10:02:00Z".to_string(),
            },
        ]);

        assert_eq!(top_paths[0].path, "/sitemap.xml");
        assert_eq!(top_paths[0].hits, 2);
        assert_eq!(top_paths[0].last_status, 304);
        assert_eq!(top_paths[1].path, "/");
        assert_eq!(top_paths[1].hits, 1);
    }
}
