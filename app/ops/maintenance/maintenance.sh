#!/bin/sh

set -eu

CONFIG_FILE="${GALLERY_APP_MAINTENANCE_CONFIG:-/usr/local/etc/gallery-app-maintenance.conf}"
ALERT_LIB="${GALLERY_APP_ALERT_LIB:-/usr/local/libexec/gallery-app-alert-lib.sh}"

if [ ! -r "${CONFIG_FILE}" ]; then
    echo "Missing maintenance config: ${CONFIG_FILE}" >&2
    exit 1
fi

. "${CONFIG_FILE}"

umask 077

alert_enabled=0
last_step="initializing"
if [ -r "${ALERT_LIB}" ]; then
    # shellcheck disable=SC1090
    . "${ALERT_LIB}"
    alert_enabled=1
fi

emit_maintenance_alert() {
    [ "${alert_enabled}" -eq 1 ] || return 0
    gallery_app_emit_alert "$@"
}

install -d -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${OPS_STATUS_DIR}"
touch "${LOG_FILE}"
chown root:wheel "${LOG_FILE}"
chmod 640 "${LOG_FILE}"
exec >> "${LOG_FILE}" 2>&1

timestamp="$(date +%Y%m%d-%H%M%S)"
recorded_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

alerts_lines=0
admin_audit_lines=0
crawler_hit_lines=0
rebuild_history_lines=0
smoke_history_lines=0
rollback_history_lines=0
removed_rehearsal_dirs=0
removed_pre_restore_dirs=0
removed_dedupe_files=0
disk_free_bytes=0
disk_free_gb=0
disk_status="healthy"

write_ops_status() {
    target_file="$1"
    tmp_file="${OPS_STATUS_DIR}/.$(basename "${target_file}").tmp"
    cat > "${tmp_file}"
    chown "${APP_USER}:${APP_GROUP}" "${tmp_file}"
    chmod 644 "${tmp_file}"
    mv -f "${tmp_file}" "${target_file}"
}

trim_ndjson_file() {
    path="$1"
    max_lines="$2"
    owner="$3"
    group="$4"
    mode="$5"

    if [ ! -f "${path}" ]; then
        printf '0\n'
        return 0
    fi

    tmp_file="${path}.tmp.$$"
    tail -n "${max_lines}" "${path}" > "${tmp_file}"
    chown "${owner}:${group}" "${tmp_file}"
    chmod "${mode}" "${tmp_file}"
    mv -f "${tmp_file}" "${path}"
    wc -l < "${path}" | awk '{print $1}'
}

count_removed_dirs() {
    path="$1"
    retention_days="$2"
    if [ ! -d "${path}" ]; then
        printf '0\n'
        return 0
    fi

    count="$(find "${path}" -mindepth 1 -maxdepth 1 -type d -mtime +"${retention_days}" | wc -l | awk '{print $1}')"
    if [ "${count}" -gt 0 ]; then
        find "${path}" -mindepth 1 -maxdepth 1 -type d -mtime +"${retention_days}" -exec rm -rf {} +
    fi
    printf '%s\n' "${count}"
}

count_removed_files() {
    path="$1"
    retention_days="$2"
    if [ ! -d "${path}" ]; then
        printf '0\n'
        return 0
    fi

    count="$(find "${path}" -mindepth 1 -maxdepth 1 -type f -mtime +"${retention_days}" | wc -l | awk '{print $1}')"
    if [ "${count}" -gt 0 ]; then
        find "${path}" -mindepth 1 -maxdepth 1 -type f -mtime +"${retention_days}" -delete
    fi
    printf '%s\n' "${count}"
}

finish() {
    status=$?

    if [ "${status}" -ne 0 ]; then
        emit_maintenance_alert \
            "error" \
            "maintenance" \
            "maintenance_failed" \
            "gallery-app maintenance failed" \
            "timestamp=${timestamp}; step=${last_step}; status=${status}" \
            "maintenance-failed:${timestamp}" \
            "yes" || true

        write_ops_status "${OPS_STATUS_DIR}/latest-maintenance.json" <<EOF || true
{
  "status": "failed",
  "recorded_at": "${recorded_at}",
  "alerts_lines": ${alerts_lines},
  "admin_audit_lines": ${admin_audit_lines},
  "crawler_hit_lines": ${crawler_hit_lines},
  "rebuild_history_lines": ${rebuild_history_lines},
  "smoke_history_lines": ${smoke_history_lines},
  "rollback_history_lines": ${rollback_history_lines},
  "removed_rehearsal_dirs": ${removed_rehearsal_dirs},
  "removed_pre_restore_dirs": ${removed_pre_restore_dirs},
  "removed_dedupe_files": ${removed_dedupe_files},
  "disk_free_bytes": ${disk_free_bytes},
  "disk_free_gb": ${disk_free_gb},
  "disk_threshold_gb": ${DISK_FREE_GB_THRESHOLD},
  "disk_status": "${disk_status}",
  "failed_step": "${last_step}"
}
EOF
    fi

    exit "${status}"
}

trap finish EXIT INT TERM HUP

echo "[$(date)] starting gallery-app maintenance ${timestamp}"

last_step="trim-alert-spool"
alerts_lines="$(trim_ndjson_file "${ALERT_SPOOL_FILE}" "${ALERT_SPOOL_MAX_LINES}" "${APP_USER}" "${APP_GROUP}" 640)"

last_step="trim-admin-audit"
admin_audit_lines="$(trim_ndjson_file "${ADMIN_AUDIT_LOG}" "${ADMIN_AUDIT_MAX_LINES}" "${APP_USER}" "${APP_GROUP}" 640)"

last_step="trim-crawler-hits"
crawler_hit_lines="$(trim_ndjson_file "${CRAWLER_HIT_LOG}" "${CRAWLER_HIT_MAX_LINES}" "${APP_USER}" "${APP_GROUP}" 640)"

last_step="trim-rebuild-history"
rebuild_history_lines="$(trim_ndjson_file "${REBUILD_HISTORY_FILE}" "${REBUILD_HISTORY_MAX_LINES}" "${APP_USER}" "${APP_GROUP}" 644)"

last_step="trim-smoke-history"
smoke_history_lines="$(trim_ndjson_file "${SMOKE_HISTORY_FILE}" "${SMOKE_HISTORY_MAX_LINES}" "${APP_USER}" "${APP_GROUP}" 644)"

last_step="trim-rollback-history"
rollback_history_lines="$(trim_ndjson_file "${ROLLBACK_HISTORY_FILE}" "${ROLLBACK_HISTORY_MAX_LINES}" "${APP_USER}" "${APP_GROUP}" 644)"

last_step="purge-rehearsals"
removed_rehearsal_dirs="$(count_removed_dirs "${REHEARSAL_ROOT}" "${REHEARSAL_RETENTION_DAYS}")"

last_step="purge-pre-restore"
removed_pre_restore_dirs="$(count_removed_dirs "${PRE_RESTORE_ROOT}" "${PRE_RESTORE_RETENTION_DAYS}")"

last_step="purge-dedupe"
removed_dedupe_files="$(count_removed_files "${ALERT_DEDUPE_DIR}" "${ALERT_DEDUPE_RETENTION_DAYS}")"

last_step="check-disk"
disk_free_kb="$(df -k "${APP_ROOT}" | awk 'NR==2 {print $4}')"
case "${disk_free_kb}" in
    ''|*[!0-9]*)
        disk_free_kb=0
        ;;
esac
disk_free_bytes=$((disk_free_kb * 1024))
disk_free_gb=$((disk_free_bytes / 1024 / 1024 / 1024))
if [ "${disk_free_gb}" -lt "${DISK_FREE_GB_THRESHOLD}" ]; then
    disk_status="low"
    emit_maintenance_alert \
        "error" \
        "maintenance" \
        "disk_low" \
        "gallery-app disk free space is below threshold" \
        "app_root=${APP_ROOT}; disk_free_gb=${disk_free_gb}; threshold_gb=${DISK_FREE_GB_THRESHOLD}" \
        "disk-low:${APP_ROOT}" \
        "yes" || true
fi

last_step="write-status"
write_ops_status "${OPS_STATUS_DIR}/latest-maintenance.json" <<EOF
{
  "status": "success",
  "recorded_at": "${recorded_at}",
  "alerts_lines": ${alerts_lines},
  "admin_audit_lines": ${admin_audit_lines},
  "crawler_hit_lines": ${crawler_hit_lines},
  "rebuild_history_lines": ${rebuild_history_lines},
  "smoke_history_lines": ${smoke_history_lines},
  "rollback_history_lines": ${rollback_history_lines},
  "removed_rehearsal_dirs": ${removed_rehearsal_dirs},
  "removed_pre_restore_dirs": ${removed_pre_restore_dirs},
  "removed_dedupe_files": ${removed_dedupe_files},
  "disk_free_bytes": ${disk_free_bytes},
  "disk_free_gb": ${disk_free_gb},
  "disk_threshold_gb": ${DISK_FREE_GB_THRESHOLD},
  "disk_status": "${disk_status}"
}
EOF

echo "[$(date)] maintenance completed"

emit_maintenance_alert \
    "info" \
    "maintenance" \
    "maintenance_success" \
    "gallery-app maintenance completed" \
    "alerts_lines=${alerts_lines}; admin_audit_lines=${admin_audit_lines}; crawler_hit_lines=${crawler_hit_lines}; disk_status=${disk_status}; disk_free_gb=${disk_free_gb}" \
    "maintenance-success:${timestamp}" \
    "no" || true
