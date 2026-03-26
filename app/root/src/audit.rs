use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use crate::{alerting::spawn_runtime_alert, auth::session_user_id, state::AppState};

pub const ADMIN_AUDIT_LOG_PATH: &str = "run/admin-audit.ndjson";

#[derive(Clone, Deserialize, Serialize)]
pub struct AdminAuditEntry {
    pub timestamp: String,
    pub actor: String,
    pub action: String,
    pub status: String,
    pub source_ip: String,
    pub request_path: String,
    pub detail: String,
}

pub fn audit_actor_from_session(state: &AppState, jar: &CookieJar) -> String {
    session_user_id(state, jar).unwrap_or_else(|| "unknown".to_string())
}

pub fn record_admin_audit(
    actor: impl Into<String>,
    action: &str,
    status: &str,
    source_ip: &str,
    request_path: &str,
    detail: impl Into<String>,
) {
    let entry = AdminAuditEntry {
        timestamp: Utc::now().to_rfc3339(),
        actor: actor.into(),
        action: action.to_string(),
        status: status.to_string(),
        source_ip: source_ip.to_string(),
        request_path: request_path.to_string(),
        detail: detail.into(),
    };

    if let Err(error) = append_admin_audit(&entry) {
        tracing::warn!(%error, "failed to append admin audit entry");
        spawn_runtime_alert(
            "error",
            "audit",
            "admin_audit_append_failed",
            "Admin audit append failed",
            error.to_string(),
        );
    }
}

pub fn load_recent_admin_audit(limit: usize) -> Vec<AdminAuditEntry> {
    let path = Path::new(ADMIN_AUDIT_LOG_PATH);
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut entries = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<AdminAuditEntry>(line).ok())
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| parse_timestamp(&right.timestamp).cmp(&parse_timestamp(&left.timestamp)));
    entries.truncate(limit);
    entries
}

fn append_admin_audit(entry: &AdminAuditEntry) -> std::io::Result<()> {
    let path = Path::new(ADMIN_AUDIT_LOG_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let payload = serde_json::to_string(entry)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
    file.write_all(payload.as_bytes())?;
    file.write_all(b"\n")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o640))?;
    }

    Ok(())
}

fn parse_timestamp(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_test_cwd;
    use std::fs;

    #[test]
    fn load_recent_admin_audit_sorts_desc_and_limits() {
        with_test_cwd("audit-sort", |_| {
            fs::create_dir_all("run").unwrap();
            fs::write(
                ADMIN_AUDIT_LOG_PATH,
                concat!(
                    "{\"timestamp\":\"2026-03-21T01:00:00Z\",\"actor\":\"a\",\"action\":\"one\",\"status\":\"success\",\"source_ip\":\"1.1.1.1\",\"request_path\":\"/a\",\"detail\":\"first\"}\n",
                    "{\"timestamp\":\"2026-03-21T03:00:00Z\",\"actor\":\"b\",\"action\":\"two\",\"status\":\"success\",\"source_ip\":\"1.1.1.2\",\"request_path\":\"/b\",\"detail\":\"second\"}\n",
                    "{\"timestamp\":\"2026-03-21T02:00:00Z\",\"actor\":\"c\",\"action\":\"three\",\"status\":\"failed\",\"source_ip\":\"1.1.1.3\",\"request_path\":\"/c\",\"detail\":\"third\"}\n"
                ),
            )
            .unwrap();

            let entries = load_recent_admin_audit(2);
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].action, "two");
            assert_eq!(entries[1].action, "three");
        });
    }

    #[test]
    fn record_admin_audit_appends_ndjson_file() {
        with_test_cwd("audit-append", |_| {
            record_admin_audit("admin", "login", "success", "127.0.0.1", "/api/login", "ok");
            record_admin_audit("admin", "logout", "success", "127.0.0.1", "/api/logout", "ok");

            let text = fs::read_to_string(ADMIN_AUDIT_LOG_PATH).unwrap();
            let lines = text.lines().collect::<Vec<_>>();
            assert_eq!(lines.len(), 2);
            assert!(lines[0].contains("\"action\":\"login\""));
            assert!(lines[1].contains("\"action\":\"logout\""));

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = fs::metadata(ADMIN_AUDIT_LOG_PATH).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o640);
            }
        });
    }
}
