#!/bin/sh

set -eu

CONFIG_FILE="${GALLERY_APP_BACKUP_CONFIG:-/usr/local/etc/gallery-app-backup.conf}"
ALERT_LIB="${GALLERY_APP_ALERT_LIB:-/usr/local/libexec/gallery-app-alert-lib.sh}"

if [ ! -r "${CONFIG_FILE}" ]; then
    echo "Missing backup config: ${CONFIG_FILE}" >&2
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

emit_backup_alert() {
    [ "${alert_enabled}" -eq 1 ] || return 0
    gallery_app_emit_alert "$@"
}

install -d -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${OPS_STATUS_DIR}"

write_ops_status() {
    target_file="$1"
    tmp_file="${OPS_STATUS_DIR}/.$(basename "${target_file}").tmp"
    cat > "${tmp_file}"
    chown "${APP_USER}:${APP_GROUP}" "${tmp_file}"
    chmod 644 "${tmp_file}"
    mv -f "${tmp_file}" "${target_file}"
}

install -d -o root -g wheel -m 750 "${BACKUP_ROOT}" "${SNAPSHOT_DIR}" "${PRE_RESTORE_DIR}"
touch "${LOG_FILE}"
chown root:wheel "${LOG_FILE}"
chmod 640 "${LOG_FILE}"

exec >> "${LOG_FILE}" 2>&1

timestamp="$(date +%Y%m%d-%H%M%S)"
snapshot_dir="${SNAPSHOT_DIR}/${timestamp}"
lock_dir="${BACKUP_ROOT}/.lock"
tmp_db_json="$(mktemp "${TMP_DIR}/gallery-app-db.XXXXXX")"
tmp_upload_list="$(mktemp "${TMP_DIR}/gallery-app-uploads.XXXXXX")"
tmp_thumb_list="$(mktemp "${TMP_DIR}/gallery-app-thumbs.XXXXXX")"
backup_complete=0

cleanup() {
    rm -f "${tmp_db_json}" "${tmp_upload_list}" "${tmp_thumb_list}"
    if [ "${backup_complete}" -ne 1 ] && [ -d "${snapshot_dir}" ]; then
        rm -rf "${snapshot_dir}"
    fi
    rmdir "${lock_dir}" 2>/dev/null || true
}

finish() {
    status=$?

    if [ "${status}" -ne 0 ]; then
        emit_backup_alert \
            "error" \
            "backup" \
            "backup_failed" \
            "gallery-app backup failed" \
            "snapshot_id=${timestamp:-unknown}; step=${last_step}; status=${status}" \
            "backup-failed:${timestamp:-unknown}" \
            "yes" || true

        write_ops_status "${OPS_STATUS_DIR}/latest-backup.json" <<EOF || true
{
  "status": "failed",
  "recorded_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "snapshot_id": "${timestamp:-unknown}",
  "snapshot_dir": "${snapshot_dir:-unknown}",
  "failed_step": "${last_step}",
  "exit_status": ${status}
}
EOF
    fi

    cleanup
    exit "${status}"
}

trap finish EXIT INT TERM HUP

if ! mkdir "${lock_dir}" 2>/dev/null; then
    echo "[$(date)] backup already in progress"
    exit 1
fi

echo "[$(date)] starting gallery-app backup ${timestamp}"

last_step="create-snapshot-dir"
install -d -o root -g wheel -m 750 "${snapshot_dir}"

last_step="backup-db"
db_summary="$(cd "${APP_ROOT}" && "${DB_TOOL}" backup-db "${tmp_db_json}")"
printf '%s\n' "${db_summary}" > "${snapshot_dir}/db-summary.json"
zstd -T0 -19 -q -o "${snapshot_dir}/db.json.zst" "${tmp_db_json}"

last_step="archive-uploads"
tar -C "${APP_ROOT}" -cf - uploads | /usr/bin/zstd -T0 -19 -q -o "${snapshot_dir}/uploads.tar.zst"

last_step="count-files"
find "${UPLOADS_DIR}" -maxdepth 1 -type f -print | sort > "${tmp_upload_list}"
find "${UPLOADS_DIR}/thumbs" -maxdepth 1 -type f -print | sort > "${tmp_thumb_list}"
upload_count="$(wc -l < "${tmp_upload_list}" | awk '{print $1}')"
thumb_count="$(wc -l < "${tmp_thumb_list}" | awk '{print $1}')"

last_step="write-manifest"
cat > "${snapshot_dir}/manifest.json" <<EOF
{
  "snapshot_id": "${timestamp}",
  "created_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "host": "$(hostname -s 2>/dev/null || hostname)",
  "db_backup_file": "db.json.zst",
  "uploads_backup_file": "uploads.tar.zst",
  "uploads_original_count": ${upload_count},
  "uploads_thumb_count": ${thumb_count},
  "db_summary": ${db_summary}
}
EOF

last_step="write-status"
write_ops_status "${OPS_STATUS_DIR}/latest-backup.json" <<EOF
{
  "status": "success",
  "recorded_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "snapshot_id": "${timestamp}",
  "snapshot_dir": "${snapshot_dir}",
  "uploads_original_count": ${upload_count},
  "uploads_thumb_count": ${thumb_count},
  "db_summary": ${db_summary}
}
EOF

last_step="refresh-latest-link"
ln -sfn "${snapshot_dir}" "${BACKUP_ROOT}/latest"

last_step="purge-old-snapshots"
find "${SNAPSHOT_DIR}" -mindepth 1 -maxdepth 1 -type d -mtime +"${RETENTION_DAYS}" -exec rm -rf {} +

backup_complete=1
echo "[$(date)] backup completed at ${snapshot_dir}"

emit_backup_alert \
    "info" \
    "backup" \
    "backup_success" \
    "gallery-app backup completed" \
    "snapshot_id=${timestamp}; uploads_original_count=${upload_count}; uploads_thumb_count=${thumb_count}" \
    "backup-success:${timestamp}" \
    "no" || true
