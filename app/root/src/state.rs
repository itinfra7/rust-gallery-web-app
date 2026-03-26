use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::broadcast;
use crate::{alerting::spawn_runtime_alert, rate_limit::{RateLimitDecision, RateLimitPolicy, RateLimitStore}};

const MAX_DASHBOARD_EVENTS: usize = 64;
const MAX_DASHBOARD_ALERTS: usize = 64;
const MAX_CRAWLER_HITS: usize = 1024;
const MAX_UPLOAD_JOBS: usize = 24;
pub const SESSION_STORE_PATH: &str = "run/admin-sessions.json";
pub const CRAWLER_HIT_STORE_PATH: &str = "run/crawler-hits.ndjson";
pub const OPS_STATUS_DIR: &str = "run/ops-status";

#[derive(Clone, Serialize, Deserialize)]
pub struct DashboardLiveEvent {
    pub kind: String,
    pub label: String,
    pub detail: String,
    pub timestamp: String,
    pub page_url: String,
    pub thumb_url: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DashboardAlert {
    pub severity: String,
    pub source: String,
    pub label: String,
    pub detail: String,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DashboardCrawlerHit {
    pub crawler: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DashboardUploadJob {
    pub upload_id: String,
    pub file_name: String,
    pub state: String,
    pub phase: String,
    pub message: String,
    pub started_at: String,
    pub updated_at: String,
    pub page_url: String,
    pub image_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    pub upload_status: Arc<Mutex<HashMap<String, String>>>,
    pub app_started_at: Instant,
    pub dashboard_events: Arc<Mutex<VecDeque<DashboardLiveEvent>>>,
    pub dashboard_alerts: Arc<Mutex<VecDeque<DashboardAlert>>>,
    pub crawler_hits: Arc<Mutex<VecDeque<DashboardCrawlerHit>>>,
    pub upload_jobs: Arc<Mutex<VecDeque<DashboardUploadJob>>>,
    pub rate_limits: Arc<Mutex<RateLimitStore>>,
    pub activity_stream_tx: broadcast::Sender<String>,
}

impl AppState {
    pub fn new(db: PgPool) -> Self {
        let (activity_stream_tx, _) = broadcast::channel(128);
        Self {
            db,
            sessions: Arc::new(Mutex::new(load_persisted_sessions())),
            upload_status: Arc::new(Mutex::new(HashMap::new())),
            app_started_at: Instant::now(),
            dashboard_events: Arc::new(Mutex::new(VecDeque::new())),
            dashboard_alerts: Arc::new(Mutex::new(VecDeque::new())),
            crawler_hits: Arc::new(Mutex::new(load_persisted_crawler_hits())),
            upload_jobs: Arc::new(Mutex::new(VecDeque::new())),
            rate_limits: Arc::new(Mutex::new(RateLimitStore::default())),
            activity_stream_tx,
        }
    }

    pub fn publish_live_event(&self, event: DashboardLiveEvent) {
        let payload = serde_json::to_string(&event).ok();
        let mut events = self.dashboard_events.lock().unwrap();
        push_bounded_front(&mut events, event, MAX_DASHBOARD_EVENTS);
        drop(events);

        if let Some(payload) = payload {
            let _ = self.activity_stream_tx.send(payload);
        }
    }

    pub fn recent_live_events(&self) -> Vec<DashboardLiveEvent> {
        self.dashboard_events.lock().unwrap().iter().cloned().collect()
    }

    pub fn push_alert(&self, alert: DashboardAlert) {
        let mut alerts = self.dashboard_alerts.lock().unwrap();
        push_bounded_front(&mut alerts, alert, MAX_DASHBOARD_ALERTS);
    }

    pub fn recent_alerts(&self) -> Vec<DashboardAlert> {
        self.dashboard_alerts.lock().unwrap().iter().cloned().collect()
    }

    pub fn record_crawler_hit(&self, hit: DashboardCrawlerHit) {
        let mut hits = self.crawler_hits.lock().unwrap();
        let persisted_hit = hit.clone();
        push_bounded_front(&mut hits, hit, MAX_CRAWLER_HITS);
        drop(hits);

        persist_crawler_hit_with_log(&persisted_hit);
    }

    pub fn recent_crawler_hits(&self) -> Vec<DashboardCrawlerHit> {
        self.crawler_hits.lock().unwrap().iter().cloned().collect()
    }

    pub fn upsert_upload_job(&self, job: DashboardUploadJob) {
        let mut jobs = self.upload_jobs.lock().unwrap();
        if let Some(index) = jobs.iter().position(|entry| entry.upload_id == job.upload_id) {
            jobs.remove(index);
        }
        push_bounded_front(&mut jobs, job, MAX_UPLOAD_JOBS);
    }

    pub fn recent_upload_jobs(&self) -> Vec<DashboardUploadJob> {
        self.upload_jobs.lock().unwrap().iter().cloned().collect()
    }

    pub fn subscribe_activity_stream(&self) -> broadcast::Receiver<String> {
        self.activity_stream_tx.subscribe()
    }

    pub fn insert_session(&self, session_id: String, session: Session) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(session_id, session);
        persist_sessions_with_log(&sessions);
    }

    pub fn remove_session(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.remove(session_id).is_some() {
            persist_sessions_with_log(&sessions);
        }
    }

    pub fn active_session_count(&self) -> i64 {
        let now = Utc::now();
        let mut sessions = self.sessions.lock().unwrap();
        let before = sessions.len();
        sessions.retain(|_, session| session.expires_at > now);
        if sessions.len() != before {
            persist_sessions_with_log(&sessions);
        }
        sessions.len() as i64
    }

    pub fn remove_expired_sessions(&self) -> usize {
        let now = Utc::now();
        let mut sessions = self.sessions.lock().unwrap();
        let before = sessions.len();
        sessions.retain(|_, session| session.expires_at > now);
        let removed = before.saturating_sub(sessions.len());
        if removed > 0 {
            persist_sessions_with_log(&sessions);
        }
        removed
    }

    pub fn persist_sessions(&self) {
        let sessions = self.sessions.lock().unwrap();
        persist_sessions_with_log(&sessions);
    }

    pub fn check_rate_limit(&self, key: &str, policy: RateLimitPolicy) -> RateLimitDecision {
        self.rate_limits
            .lock()
            .unwrap()
            .check_block(key, policy)
    }

    pub fn record_rate_limit_event(&self, key: &str, policy: RateLimitPolicy) -> RateLimitDecision {
        self.rate_limits
            .lock()
            .unwrap()
            .record_event(key, policy)
    }

    pub fn reset_rate_limit(&self, key: &str) {
        self.rate_limits.lock().unwrap().reset(key);
    }
}

fn push_bounded_front<T>(queue: &mut VecDeque<T>, value: T, limit: usize) {
    queue.push_front(value);
    while queue.len() > limit {
        queue.pop_back();
    }
}

fn load_persisted_sessions() -> HashMap<String, Session> {
    let path = Path::new(SESSION_STORE_PATH);
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return HashMap::new(),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "failed to read persisted admin sessions"
            );
            spawn_runtime_alert(
                "warn",
                "sessions",
                "session_store_read_failed",
                "Persisted admin session load failed",
                format!("path={} error={}", path.display(), error),
            );
            return HashMap::new();
        }
    };

    let mut sessions = match serde_json::from_str::<HashMap<String, Session>>(&text) {
        Ok(sessions) => sessions,
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "failed to parse persisted admin sessions"
            );
            spawn_runtime_alert(
                "warn",
                "sessions",
                "session_store_parse_failed",
                "Persisted admin session parse failed",
                format!("path={} error={}", path.display(), error),
            );
            return HashMap::new();
        }
    };

    let now = Utc::now();
    sessions.retain(|_, session| session.expires_at > now);
    sessions
}

fn load_persisted_crawler_hits() -> VecDeque<DashboardCrawlerHit> {
    let path = Path::new(CRAWLER_HIT_STORE_PATH);
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return VecDeque::new(),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "failed to read persisted crawler hits"
            );
            spawn_runtime_alert(
                "warn",
                "crawler-telemetry",
                "crawler_hit_store_read_failed",
                "Persisted crawler telemetry load failed",
                format!("path={} error={}", path.display(), error),
            );
            return VecDeque::new();
        }
    };

    let mut hits = VecDeque::new();
    let mut parse_errors = 0usize;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<DashboardCrawlerHit>(line) {
            Ok(hit) => push_bounded_front(&mut hits, hit, MAX_CRAWLER_HITS),
            Err(_) => parse_errors += 1,
        }
    }

    if parse_errors > 0 {
        tracing::warn!(
            path = %path.display(),
            parse_errors,
            "failed to parse one or more persisted crawler hits"
        );
        spawn_runtime_alert(
            "warn",
            "crawler-telemetry",
            "crawler_hit_store_parse_failed",
            "Persisted crawler telemetry parse failed",
            format!("path={} parse_errors={}", path.display(), parse_errors),
        );
    }

    hits
}

fn persist_sessions_with_log(sessions: &HashMap<String, Session>) {
    if let Err(error) = persist_sessions(sessions) {
        tracing::warn!(%error, "failed to persist admin sessions");
        spawn_runtime_alert(
            "error",
            "sessions",
            "session_store_persist_failed",
            "Persisted admin session write failed",
            error.to_string(),
        );
    }
}

fn persist_crawler_hit_with_log(hit: &DashboardCrawlerHit) {
    if let Err(error) = persist_crawler_hit(hit) {
        tracing::warn!(%error, "failed to persist crawler hit");
        spawn_runtime_alert(
            "warn",
            "crawler-telemetry",
            "crawler_hit_store_persist_failed",
            "Persisted crawler telemetry write failed",
            error.to_string(),
        );
    }
}

fn persist_sessions(sessions: &HashMap<String, Session>) -> io::Result<()> {
    let path = Path::new(SESSION_STORE_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let payload = serde_json::to_vec_pretty(sessions)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, payload)?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

fn persist_crawler_hit(hit: &DashboardCrawlerHit) -> io::Result<()> {
    let path = Path::new(CRAWLER_HIT_STORE_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let payload = serde_json::to_string(hit)
        .map_err(io::Error::other)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(payload.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{dummy_pool, with_test_cwd};
    use chrono::{Duration, Utc};

    #[tokio::test]
    async fn persisted_sessions_survive_state_reinitialization() {
        with_test_cwd("state-persist", |_| {
            let state = AppState::new(dummy_pool());
            state.insert_session(
                "session-1".to_string(),
                Session {
                    user_id: "admin".to_string(),
                    created_at: Utc::now(),
                    expires_at: Utc::now() + Duration::hours(6),
                },
            );

            let reloaded = AppState::new(dummy_pool());
            let sessions = reloaded.sessions.lock().unwrap();
            assert_eq!(sessions.get("session-1").unwrap().user_id, "admin");
        });
    }

    #[tokio::test]
    async fn active_session_count_prunes_expired_sessions() {
        with_test_cwd("state-prune", |_| {
            let state = AppState::new(dummy_pool());
            state.insert_session(
                "expired".to_string(),
                Session {
                    user_id: "old".to_string(),
                    created_at: Utc::now() - Duration::hours(2),
                    expires_at: Utc::now() - Duration::minutes(1),
                },
            );
            state.insert_session(
                "active".to_string(),
                Session {
                    user_id: "new".to_string(),
                    created_at: Utc::now(),
                    expires_at: Utc::now() + Duration::hours(1),
                },
            );

            assert_eq!(state.active_session_count(), 1);
            let sessions = state.sessions.lock().unwrap();
            assert!(sessions.contains_key("active"));
            assert!(!sessions.contains_key("expired"));
        });
    }

    #[tokio::test]
    async fn persisted_crawler_hits_survive_state_reinitialization() {
        with_test_cwd("state-crawler-persist", |_| {
            let state = AppState::new(dummy_pool());
            state.record_crawler_hit(DashboardCrawlerHit {
                crawler: "Google".to_string(),
                method: "GET".to_string(),
                path: "/".to_string(),
                status: 200,
                timestamp: "2026-03-21T00:00:00Z".to_string(),
            });
            state.record_crawler_hit(DashboardCrawlerHit {
                crawler: "Bing".to_string(),
                method: "GET".to_string(),
                path: "/sitemap.xml".to_string(),
                status: 200,
                timestamp: "2026-03-21T00:01:00Z".to_string(),
            });

            let reloaded = AppState::new(dummy_pool());
            let hits = reloaded.recent_crawler_hits();
            assert_eq!(hits.len(), 2);
            assert_eq!(hits[0].crawler, "Bing");
            assert_eq!(hits[0].path, "/sitemap.xml");
            assert_eq!(hits[1].crawler, "Google");
            assert_eq!(hits[1].path, "/");
        });
    }
}
