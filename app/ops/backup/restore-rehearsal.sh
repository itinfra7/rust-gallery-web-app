#!/bin/sh

set -eu

CONFIG_FILE="${GALLERY_APP_BACKUP_CONFIG:-/usr/local/etc/gallery-app-backup.conf}"
ALERT_LIB="${GALLERY_APP_ALERT_LIB:-/usr/local/libexec/gallery-app-alert-lib.sh}"

if [ ! -r "${CONFIG_FILE}" ]; then
    echo "Missing backup config: ${CONFIG_FILE}" >&2
    exit 1
fi

. "${CONFIG_FILE}"

alert_enabled=0
last_step="initializing"
if [ -r "${ALERT_LIB}" ]; then
    # shellcheck disable=SC1090
    . "${ALERT_LIB}"
    alert_enabled=1
fi

emit_rehearsal_alert() {
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

snapshot_arg="${1:-latest}"

case "${snapshot_arg}" in
    latest)
        snapshot_dir="${BACKUP_ROOT}/latest"
        ;;
    *)
        snapshot_dir="${snapshot_arg}"
        ;;
esac

db_backup="${snapshot_dir}/db.json.zst"
uploads_backup="${snapshot_dir}/uploads.tar.zst"
manifest_path="${snapshot_dir}/manifest.json"

[ -d "${snapshot_dir}" ] || { echo "Missing snapshot directory: ${snapshot_dir}" >&2; exit 1; }
[ -f "${db_backup}" ] || { echo "Missing database backup: ${db_backup}" >&2; exit 1; }
[ -f "${uploads_backup}" ] || { echo "Missing uploads backup: ${uploads_backup}" >&2; exit 1; }

timestamp="$(date +%Y%m%d-%H%M%S)"
rehearsal_root="${BACKUP_ROOT}/rehearsals/${timestamp}"
extract_root="${rehearsal_root}/extracted"
tmp_db_json="$(mktemp "${TMP_DIR}/gallery-app-rehearsal.XXXXXX")"

cleanup() {
    rm -f "${tmp_db_json}"
}

finish() {
    status=$?

    if [ "${status}" -ne 0 ]; then
        emit_rehearsal_alert \
            "error" \
            "restore-rehearsal" \
            "restore_rehearsal_failed" \
            "gallery-app restore rehearsal failed" \
            "snapshot_dir=${snapshot_dir:-unknown}; step=${last_step}; status=${status}" \
            "restore-rehearsal-failed:${snapshot_arg}" \
            "yes" || true

        write_ops_status "${OPS_STATUS_DIR}/latest-restore-rehearsal.json" <<EOF || true
{
  "status": "failed",
  "recorded_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
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

last_step="prepare-output"
install -d -o root -g wheel -m 750 "${BACKUP_ROOT}/rehearsals" "${rehearsal_root}" "${extract_root}"

last_step="decompress-db"
/usr/bin/zstd -dqc "${db_backup}" > "${tmp_db_json}"
last_step="rehearse-db"
(cd "${APP_ROOT}" && "${DB_TOOL}" rehearse-db-restore "${tmp_db_json}") > "${rehearsal_root}/db-rehearsal.json"

last_step="extract-uploads"
/usr/bin/zstd -dqc "${uploads_backup}" | tar -C "${extract_root}" -xf -

last_step="count-files"
upload_count="$(find "${extract_root}/uploads" -maxdepth 1 -type f | wc -l | awk '{print $1}')"
thumb_count="$(find "${extract_root}/uploads/thumbs" -maxdepth 1 -type f | wc -l | awk '{print $1}')"

manifest_upload_count=""
manifest_thumb_count=""
if [ -f "${manifest_path}" ]; then
    manifest_upload_count="$(sed -n 's/.*"uploads_original_count": \([0-9][0-9]*\),/\1/p' "${manifest_path}" | head -n 1)"
    manifest_thumb_count="$(sed -n 's/.*"uploads_thumb_count": \([0-9][0-9]*\),/\1/p' "${manifest_path}" | head -n 1)"
fi

last_step="write-summary"
cat > "${rehearsal_root}/summary.json" <<EOF
{
  "snapshot_dir": "${snapshot_dir}",
  "rehearsal_dir": "${rehearsal_root}",
  "uploads_original_count": ${upload_count},
  "uploads_thumb_count": ${thumb_count},
  "manifest_upload_count": ${manifest_upload_count:-null},
  "manifest_thumb_count": ${manifest_thumb_count:-null}
}
EOF

if [ -n "${manifest_upload_count}" ] && [ "${upload_count}" != "${manifest_upload_count}" ]; then
    echo "Upload file count mismatch during rehearsal." >&2
    exit 1
fi

if [ -n "${manifest_thumb_count}" ] && [ "${thumb_count}" != "${manifest_thumb_count}" ]; then
    echo "Thumb file count mismatch during rehearsal." >&2
    exit 1
fi

db_rehearsal_json="$(cat "${rehearsal_root}/db-rehearsal.json")"

last_step="write-status"
write_ops_status "${OPS_STATUS_DIR}/latest-restore-rehearsal.json" <<EOF
{
  "status": "success",
  "recorded_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "snapshot_dir": "${snapshot_dir}",
  "rehearsal_dir": "${rehearsal_root}",
  "uploads_original_count": ${upload_count},
  "uploads_thumb_count": ${thumb_count},
  "manifest_upload_count": ${manifest_upload_count:-null},
  "manifest_thumb_count": ${manifest_thumb_count:-null},
  "db_rehearsal": ${db_rehearsal_json}
}
EOF

echo "Restore rehearsal completed successfully."
echo "Snapshot: ${snapshot_dir}"
echo "Rehearsal output: ${rehearsal_root}"

emit_rehearsal_alert \
    "info" \
    "restore-rehearsal" \
    "restore_rehearsal_success" \
    "gallery-app restore rehearsal completed" \
    "snapshot_dir=${snapshot_dir}; uploads_original_count=${upload_count}; uploads_thumb_count=${thumb_count}" \
    "restore-rehearsal-success:${snapshot_arg}" \
    "no" || true
