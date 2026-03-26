#!/bin/sh

set -eu

SERVICE_NAME="gallery_app"
APP_ROOT="<APP_ROOT>"
APP_USER="<APP_USER>"
APP_GROUP="<APP_GROUP>"
RUNTIME_DIR="${APP_ROOT}/bin"
RUNTIME_BINARY="${RUNTIME_DIR}/gallery-app"
BACKUP_DIR="${RUNTIME_DIR}/backups"
OPS_STATUS_DIR="${APP_ROOT}/run/ops-status"
LATEST_ROLLBACK_FILE="${OPS_STATUS_DIR}/latest-rollback.json"
ROLLBACK_HISTORY_FILE="${OPS_STATUS_DIR}/rollback-history.ndjson"
SERVICE_BIN="/usr/sbin/service"
SMOKE_SCRIPT="${GALLERY_APP_SMOKE_SCRIPT:-/usr/local/libexec/gallery-app-smoke-check.sh}"
ALERT_LIB="${GALLERY_APP_ALERT_LIB:-/usr/local/libexec/gallery-app-alert-lib.sh}"

timestamp=$(date +%Y%m%d-%H%M%S)
start_epoch=$(date +%s)
target_arg="${1:-latest}"
target_binary=""
staged_binary="${RUNTIME_DIR}/gallery-app.rollback.${timestamp}"
current_backup="${BACKUP_DIR}/gallery-app.rollback-current.${timestamp}"
was_running=0
service_action="start"
alert_enabled=0

if [ -r "${ALERT_LIB}" ]; then
    # shellcheck disable=SC1090
    . "${ALERT_LIB}"
    alert_enabled=1
fi

emit_rollback_alert() {
    [ "${alert_enabled}" -eq 1 ] || return 0
    gallery_app_emit_alert "$@"
}

write_rollback_record() {
    status="$1"
    detail="$2"
    recorded_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    duration_seconds=$(( $(date +%s) - start_epoch ))
    record=$(cat <<EOF
{"status":"${status}","recorded_at":"${recorded_at}","duration_seconds":${duration_seconds},"service_action":"${service_action}","target_binary":"${target_binary}","detail":"${detail}"}
EOF
)

    install -d -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${OPS_STATUS_DIR}"

    latest_tmp="${OPS_STATUS_DIR}/.latest-rollback.json.tmp"
    printf '%s\n' "${record}" > "${latest_tmp}"
    chown "${APP_USER}:${APP_GROUP}" "${latest_tmp}"
    chmod 644 "${latest_tmp}"
    mv -f "${latest_tmp}" "${LATEST_ROLLBACK_FILE}"

    history_tmp="${OPS_STATUS_DIR}/.rollback-history.ndjson.tmp"
    {
        if [ -f "${ROLLBACK_HISTORY_FILE}" ]; then
            cat "${ROLLBACK_HISTORY_FILE}"
        fi
        printf '%s\n' "${record}"
    } | tail -n 32 > "${history_tmp}"
    chown "${APP_USER}:${APP_GROUP}" "${history_tmp}"
    chmod 644 "${history_tmp}"
    mv -f "${history_tmp}" "${ROLLBACK_HISTORY_FILE}"
}

if [ ! -x "${RUNTIME_BINARY}" ]; then
    echo "${RUNTIME_BINARY} is missing or not executable." >&2
    exit 1
fi

install -d -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${BACKUP_DIR}"
install -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${RUNTIME_BINARY}" "${current_backup}"

case "${target_arg}" in
    latest)
        target_binary="$(ls -1t "${BACKUP_DIR}"/gallery-app.* 2>/dev/null | grep -v 'rollback-current' | head -n 1 || true)"
        ;;
    *)
        target_binary="${target_arg}"
        ;;
esac

if [ -z "${target_binary}" ] || [ ! -f "${target_binary}" ]; then
    echo "No rollback target found." >&2
    write_rollback_record "failed" "rollback-target-missing"
    emit_rollback_alert \
        "error" \
        "rollback" \
        "rollback_failed" \
        "gallery_app rollback failed because no rollback target was available" \
        "timestamp=${timestamp}; target=${target_arg}" \
        "rollback-failed:${timestamp}" \
        "yes" || true
    exit 1
fi

install -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${target_binary}" "${staged_binary}"
mv -f "${staged_binary}" "${RUNTIME_BINARY}"

if ${SERVICE_BIN} "${SERVICE_NAME}" onestatus >/dev/null 2>&1; then
    was_running=1
    service_action="restart"
fi

if [ "${was_running}" -eq 1 ]; then
    if ! ${SERVICE_BIN} "${SERVICE_NAME}" restart; then
        install -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${current_backup}" "${staged_binary}"
        mv -f "${staged_binary}" "${RUNTIME_BINARY}"
        ${SERVICE_BIN} "${SERVICE_NAME}" start || true
        write_rollback_record "failed" "service-restart-failed"
        emit_rollback_alert \
            "error" \
            "rollback" \
            "rollback_failed" \
            "gallery_app rollback failed during service restart" \
            "timestamp=${timestamp}; target_binary=${target_binary}" \
            "rollback-failed:${timestamp}" \
            "yes" || true
        exit 1
    fi
else
    if ! ${SERVICE_BIN} "${SERVICE_NAME}" start; then
        install -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${current_backup}" "${staged_binary}"
        mv -f "${staged_binary}" "${RUNTIME_BINARY}"
        write_rollback_record "failed" "service-start-failed"
        emit_rollback_alert \
            "error" \
            "rollback" \
            "rollback_failed" \
            "gallery_app rollback failed during service start" \
            "timestamp=${timestamp}; target_binary=${target_binary}" \
            "rollback-failed:${timestamp}" \
            "yes" || true
        exit 1
    fi
fi

if ! "${SMOKE_SCRIPT}" "http://127.0.0.1:8080"; then
    install -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${current_backup}" "${staged_binary}"
    mv -f "${staged_binary}" "${RUNTIME_BINARY}"
    ${SERVICE_BIN} "${SERVICE_NAME}" restart || ${SERVICE_BIN} "${SERVICE_NAME}" start || true
    write_rollback_record "failed" "smoke-check-failed-after-rollback"
    emit_rollback_alert \
        "error" \
        "rollback" \
        "rollback_failed" \
        "gallery_app rollback failed the post-rollback smoke check" \
        "timestamp=${timestamp}; target_binary=${target_binary}" \
        "rollback-failed:${timestamp}" \
        "yes" || true
    exit 1
fi

write_rollback_record "success" "rollback-smoke-check-completed"
emit_rollback_alert \
    "info" \
    "rollback" \
    "rollback_success" \
    "gallery_app rollback completed" \
    "timestamp=${timestamp}; target_binary=${target_binary}; service_action=${service_action}" \
    "rollback-success:${timestamp}" \
    "no" || true

echo "Rollback completed successfully."
