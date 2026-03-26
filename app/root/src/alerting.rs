use chrono::Utc;
use reqwest::Client;
use serde::Serialize;
use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
use tracing::warn;

const DEFAULT_ALERT_APP_ROOT: &str = "<APP_ROOT>";

#[derive(Clone, Debug)]
struct RuntimeAlertConfig {
    ops_status_dir: PathBuf,
    spool_file: PathBuf,
    dedupe_dir: PathBuf,
    notify_info: bool,
    dedupe_cooldown_seconds: i64,
    telegram_timeout_seconds: u64,
    telegram_bot_token: String,
    telegram_chat_id: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlertDeliveryStatus {
    Sent,
    RecordedOnly,
    Suppressed,
    SkippedConfig,
    Failed,
}

impl AlertDeliveryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::RecordedOnly => "recorded-only",
            Self::Suppressed => "suppressed",
            Self::SkippedConfig => "skipped-config",
            Self::Failed => "failed",
        }
    }
}

#[derive(Serialize)]
struct RuntimeAlertRecord {
    timestamp: String,
    host: String,
    source: String,
    severity: String,
    kind: String,
    summary: String,
    detail: String,
    dedupe_key: String,
    delivery_status: String,
}

impl RuntimeAlertConfig {
    fn from_env() -> Self {
        let app_root = env::var("ALERT_APP_ROOT")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_ALERT_APP_ROOT.to_string());

        let ops_status_dir = env::var("ALERT_OPS_STATUS_DIR")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(&app_root).join("run/ops-status"));
        let spool_file = env::var("ALERT_SPOOL_FILE")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(&app_root).join("run/alerts.ndjson"));
        let dedupe_dir = env::var("ALERT_DEDUPE_DIR")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| ops_status_dir.join("alert-dedupe"));

        let notify_info = env::var("ALERT_NOTIFY_INFO")
            .ok()
            .map(|value| value.trim().eq_ignore_ascii_case("yes"))
            .unwrap_or(false);
        let dedupe_cooldown_seconds = env::var("ALERT_DEDUPE_COOLDOWN_SECONDS")
            .ok()
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(900);
        let telegram_timeout_seconds = env::var("ALERT_TELEGRAM_TIMEOUT_SECONDS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(15);
        let telegram_bot_token = env::var("ALERT_TELEGRAM_BOT_TOKEN").unwrap_or_default();
        let telegram_chat_id = env::var("ALERT_TELEGRAM_CHAT_ID").unwrap_or_default();

        Self {
            ops_status_dir,
            spool_file,
            dedupe_dir,
            notify_info,
            dedupe_cooldown_seconds,
            telegram_timeout_seconds,
            telegram_bot_token,
            telegram_chat_id,
        }
    }

    fn kind_override(&self, kind: &str) -> Option<bool> {
        let key = sanitize_kind_key(kind);
        if key.is_empty() {
            return None;
        }

        let env_key = format!("ALERT_KIND_NOTIFY_{}", key);
        match env::var(&env_key).ok()?.trim().to_ascii_lowercase().as_str() {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        }
    }

    fn should_send(&self, severity: &str, kind: &str, force_send: bool) -> bool {
        if force_send {
            return true;
        }

        if let Some(override_value) = self.kind_override(kind) {
            return override_value;
        }

        match severity {
            "critical" | "error" | "warn" | "warning" => true,
            "info" => self.notify_info,
            _ => false,
        }
    }

    fn ensure_storage(&self) -> io::Result<()> {
        fs::create_dir_all(&self.ops_status_dir)?;
        fs::create_dir_all(&self.dedupe_dir)?;
        if let Some(parent) = self.spool_file.parent() {
            fs::create_dir_all(parent)?;
        }
        if !self.spool_file.exists() {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.spool_file)?;
            set_mode(&self.spool_file, 0o640)?;
        }
        Ok(())
    }

    fn is_suppressed(&self, dedupe_key: &str, force_send: bool) -> io::Result<bool> {
        if force_send || dedupe_key.trim().is_empty() {
            return Ok(false);
        }

        let stamp_path = self
            .dedupe_dir
            .join(format!("{:016x}.stamp", fnv1a64(dedupe_key.as_bytes())));
        let now_epoch = Utc::now().timestamp();

        if let Ok(previous) = fs::read_to_string(&stamp_path) {
            if let Ok(last_epoch) = previous.trim().parse::<i64>() {
                if now_epoch.saturating_sub(last_epoch) < self.dedupe_cooldown_seconds {
                    return Ok(true);
                }
            }
        }

        fs::write(&stamp_path, format!("{}\n", now_epoch))?;
        set_mode(&stamp_path, 0o640)?;
        Ok(false)
    }

    fn append_record(&self, record: &RuntimeAlertRecord) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.spool_file)?;
        let payload = serde_json::to_string(record)
            .map_err(io::Error::other)?;
        file.write_all(payload.as_bytes())?;
        file.write_all(b"\n")?;
        set_mode(&self.spool_file, 0o640)?;
        Ok(())
    }

    fn write_latest_status(&self, record: &RuntimeAlertRecord) -> io::Result<()> {
        let target = self.ops_status_dir.join("latest-alert-delivery.json");
        let tmp = self.ops_status_dir.join(".latest-alert-delivery.json.tmp");
        let payload = serde_json::to_vec_pretty(record)
            .map_err(io::Error::other)?;
        fs::write(&tmp, payload)?;
        set_mode(&tmp, 0o644)?;
        fs::rename(tmp, &target)?;
        set_mode(&target, 0o644)?;
        Ok(())
    }
}

pub fn spawn_runtime_alert(
    severity: impl Into<String>,
    source: impl Into<String>,
    kind: impl Into<String>,
    summary: impl Into<String>,
    detail: impl Into<String>,
) {
    spawn_runtime_alert_with_force(severity, source, kind, summary, detail, false);
}

pub fn spawn_runtime_alert_with_force(
    severity: impl Into<String>,
    source: impl Into<String>,
    kind: impl Into<String>,
    summary: impl Into<String>,
    detail: impl Into<String>,
    force_send: bool,
) {
    let severity = severity.into();
    let source = source.into();
    let kind = kind.into();
    let summary = summary.into();
    let detail = detail.into();

    let handle = match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle,
        Err(_) => {
            warn!(
                severity = %severity,
                source = %source,
                kind = %kind,
                "runtime alert skipped because no Tokio runtime is active"
            );
            return;
        }
    };

    handle.spawn(async move {
        if let Err(error) = emit_runtime_alert(severity, source, kind, summary, detail, force_send).await {
            warn!(%error, "failed to emit runtime alert");
        }
    });
}

pub async fn emit_runtime_alert(
    severity: String,
    source: String,
    kind: String,
    summary: String,
    detail: String,
    force_send: bool,
) -> io::Result<AlertDeliveryStatus> {
    let config = RuntimeAlertConfig::from_env();
    config.ensure_storage()?;

    let recorded_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let host = safe_hostname();
    let summary_compact = compact_text(&summary);
    let detail_compact = compact_text(&detail);
    let dedupe_key = format!("runtime:{}:{}", source, kind);

    let delivery_status = if config.is_suppressed(&dedupe_key, force_send)? {
        AlertDeliveryStatus::Suppressed
    } else if config.should_send(&severity, &kind, force_send) {
        let message = format!(
            "[{}] {}\nhost={}\nsource={}\nkind={}\n{}",
            severity, summary_compact, host, source, kind, detail_compact
        );
        match send_telegram(&config, &message).await {
            Ok(status) => status,
            Err(error) => {
                warn!(
                    %error,
                    source = %source,
                    kind = %kind,
                    "failed to deliver runtime alert to Telegram"
                );
                AlertDeliveryStatus::Failed
            }
        }
    } else {
        AlertDeliveryStatus::RecordedOnly
    };

    let record = RuntimeAlertRecord {
        timestamp: recorded_at,
        host,
        source,
        severity,
        kind,
        summary: summary_compact,
        detail: detail_compact,
        dedupe_key,
        delivery_status: delivery_status.as_str().to_string(),
    };

    config.append_record(&record)?;
    config.write_latest_status(&record)?;

    Ok(delivery_status)
}

async fn send_telegram(
    config: &RuntimeAlertConfig,
    message_text: &str,
) -> Result<AlertDeliveryStatus, reqwest::Error> {
    if config.telegram_bot_token.trim().is_empty() || config.telegram_chat_id.trim().is_empty() {
        return Ok(AlertDeliveryStatus::SkippedConfig);
    }

    let endpoint = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        config.telegram_bot_token.trim()
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(config.telegram_timeout_seconds))
        .build()?;

    let response = client
        .post(endpoint)
        .form(&[
            ("chat_id", config.telegram_chat_id.trim()),
            ("text", message_text),
        ])
        .send()
        .await?;

    if response.status().is_success() {
        Ok(AlertDeliveryStatus::Sent)
    } else {
        Ok(AlertDeliveryStatus::Failed)
    }
}

fn compact_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sanitize_kind_key(kind: &str) -> String {
    kind.chars()
        .map(|character| match character {
            'a'..='z' => character.to_ascii_uppercase(),
            'A'..='Z' | '0'..='9' => character,
            _ => '_',
        })
        .collect::<String>()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn safe_hostname() -> String {
    let mut buffer = [0u8; 256];
    let status = unsafe {
        libc::gethostname(
            buffer.as_mut_ptr() as *mut libc::c_char,
            buffer.len(),
        )
    };

    if status == 0 {
        let end = buffer.iter().position(|value| *value == 0).unwrap_or(buffer.len());
        let host = String::from_utf8_lossy(&buffer[..end]).trim().to_string();
        let short = host.split('.').next().unwrap_or(&host).trim();
        if !short.is_empty() {
            return short.to_string();
        }
    }

    env::var("HOSTNAME")
        .ok()
        .and_then(|value| value.split('.').next().map(str::to_string))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(mode);
        fs::set_permissions(path, permissions)?;
    }

    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }

    Ok(())
}
