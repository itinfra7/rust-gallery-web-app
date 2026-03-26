#!/bin/sh

set -eu

SERVICE_NAME="gallery_app"
APP_ROOT="<APP_ROOT>"
APP_USER="<APP_USER>"
APP_GROUP="<APP_GROUP>"
TARGET_BINARY="${APP_ROOT}/target/release/gallery-app"
TARGET_MAINT_BINARY="${APP_ROOT}/target/release/gallery-maint"
RUNTIME_DIR="${APP_ROOT}/bin"
RUNTIME_BINARY="${RUNTIME_DIR}/gallery-app"
RUNTIME_MAINT_BINARY="${RUNTIME_DIR}/gallery-maint"
BACKUP_DIR="${RUNTIME_DIR}/backups"
OPS_STATUS_DIR="${APP_ROOT}/run/ops-status"
LATEST_REBUILD_FILE="${OPS_STATUS_DIR}/latest-rebuild.json"
REBUILD_HISTORY_FILE="${OPS_STATUS_DIR}/rebuild-history.ndjson"
SERVICE_BIN="/usr/sbin/service"
ALERT_LIB="${GALLERY_APP_ALERT_LIB:-/usr/local/libexec/gallery-app-alert-lib.sh}"
SMOKE_SCRIPT="${GALLERY_APP_SMOKE_SCRIPT:-/usr/local/libexec/gallery-app-smoke-check.sh}"

timestamp=$(date +%Y%m%d-%H%M%S)
start_epoch=$(date +%s)
staged_binary="${RUNTIME_DIR}/gallery-app.new.${timestamp}"
rollback_binary="${BACKUP_DIR}/gallery-app.${timestamp}"
had_runtime_binary=0
was_running=0
service_action="start"
alert_enabled=0

if [ -r "${ALERT_LIB}" ]; then
    # shellcheck disable=SC1090
    . "${ALERT_LIB}"
    alert_enabled=1
fi

emit_rebuild_alert() {
    [ "${alert_enabled}" -eq 1 ] || return 0
    gallery_app_emit_alert "$@"
}

write_rebuild_record() {
    status="$1"
    detail="$2"
    end_epoch=$(date +%s)
    duration_seconds=$((end_epoch - start_epoch))
    recorded_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    record=$(cat <<EOF
{"status":"${status}","recorded_at":"${recorded_at}","duration_seconds":${duration_seconds},"service_action":"${service_action}","detail":"${detail}"}
EOF
)

    install -d -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${OPS_STATUS_DIR}"

    latest_tmp="${OPS_STATUS_DIR}/.latest-rebuild.json.tmp"
    printf '%s\n' "${record}" > "${latest_tmp}"
    chown "${APP_USER}:${APP_GROUP}" "${latest_tmp}"
    chmod 644 "${latest_tmp}"
    mv -f "${latest_tmp}" "${LATEST_REBUILD_FILE}"

    history_tmp="${OPS_STATUS_DIR}/.rebuild-history.ndjson.tmp"
    {
        if [ -f "${REBUILD_HISTORY_FILE}" ]; then
            cat "${REBUILD_HISTORY_FILE}"
        fi
        printf '%s\n' "${record}"
    } | tail -n 32 > "${history_tmp}"
    chown "${APP_USER}:${APP_GROUP}" "${history_tmp}"
    chmod 644 "${history_tmp}"
    mv -f "${history_tmp}" "${REBUILD_HISTORY_FILE}"
}

echo "Building release binary for ${SERVICE_NAME} as ${APP_USER}..."
if ! su - "${APP_USER}" -c "cd ${APP_ROOT} && cargo build --release"; then
    write_rebuild_record "failed" "cargo-build-failed"
    emit_rebuild_alert \
        "error" \
        "rebuild" \
        "rebuild_failed" \
        "gallery_app rebuild failed during cargo build" \
        "timestamp=${timestamp}; stage=cargo-build" \
        "rebuild-failed:${timestamp}" \
        "yes" || true
    exit 1
fi

if [ ! -x "${TARGET_BINARY}" ]; then
    echo "Build finished but ${TARGET_BINARY} is missing or not executable." >&2
    write_rebuild_record "failed" "runtime-binary-missing"
    emit_rebuild_alert \
        "error" \
        "rebuild" \
        "rebuild_failed" \
        "gallery_app rebuild failed because the runtime binary was missing" \
        "timestamp=${timestamp}; stage=runtime-binary-missing" \
        "rebuild-failed:${timestamp}" \
        "yes" || true
    exit 1
fi

install -d -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${RUNTIME_DIR}"
install -d -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${BACKUP_DIR}"

if [ ! -x "${SMOKE_SCRIPT}" ]; then
    echo "${SMOKE_SCRIPT} is missing or not executable." >&2
    write_rebuild_record "failed" "smoke-script-missing"
    emit_rebuild_alert \
        "error" \
        "rebuild" \
        "rebuild_failed" \
        "gallery_app rebuild failed because the smoke-check script was missing" \
        "timestamp=${timestamp}; stage=smoke-script-missing" \
        "rebuild-failed:${timestamp}" \
        "yes" || true
    exit 1
fi

if [ -f "${RUNTIME_BINARY}" ]; then
    had_runtime_binary=1
    install -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${RUNTIME_BINARY}" "${rollback_binary}"
fi

install -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${TARGET_BINARY}" "${staged_binary}"
mv -f "${staged_binary}" "${RUNTIME_BINARY}"

if [ -x "${TARGET_MAINT_BINARY}" ]; then
    install -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${TARGET_MAINT_BINARY}" "${RUNTIME_MAINT_BINARY}"
fi

if ${SERVICE_BIN} "${SERVICE_NAME}" onestatus >/dev/null 2>&1; then
    was_running=1
    service_action="restart"
fi

if [ "${was_running}" -eq 1 ]; then
    echo "Restarting ${SERVICE_NAME} with the new binary..."
    if ${SERVICE_BIN} "${SERVICE_NAME}" restart; then
        if "${SMOKE_SCRIPT}" "http://127.0.0.1:8080"; then
            write_rebuild_record "success" "restart-and-smoke-check-completed"
            emit_rebuild_alert \
                "info" \
                "rebuild" \
                "rebuild_success" \
                "gallery_app rebuild completed with restart" \
                "timestamp=${timestamp}; service_action=restart; smoke_check=passed" \
                "rebuild-success:${timestamp}" \
                "no" || true
            echo "Rebuild, restart, and smoke check completed successfully."
            exit 0
        fi
    fi
else
    echo "Starting ${SERVICE_NAME} with the new binary..."
    if ${SERVICE_BIN} "${SERVICE_NAME}" start; then
        if "${SMOKE_SCRIPT}" "http://127.0.0.1:8080"; then
            write_rebuild_record "success" "start-and-smoke-check-completed"
            emit_rebuild_alert \
                "info" \
                "rebuild" \
                "rebuild_success" \
                "gallery_app rebuild completed with start" \
                "timestamp=${timestamp}; service_action=start; smoke_check=passed" \
                "rebuild-success:${timestamp}" \
                "no" || true
            echo "Rebuild, start, and smoke check completed successfully."
            exit 0
        fi
    fi
fi

echo "Service start or smoke check failed after rebuild. Attempting rollback..." >&2

if [ "${had_runtime_binary}" -eq 1 ] && [ -f "${rollback_binary}" ]; then
    install -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${rollback_binary}" "${staged_binary}"
    mv -f "${staged_binary}" "${RUNTIME_BINARY}"

    if [ "${was_running}" -eq 1 ]; then
        ${SERVICE_BIN} "${SERVICE_NAME}" start || true
        "${SMOKE_SCRIPT}" "http://127.0.0.1:8080" || true
    fi
fi

write_rebuild_record "failed" "service-start-or-smoke-check-failed-after-rebuild"

emit_rebuild_alert \
    "error" \
    "rebuild" \
    "rebuild_failed" \
    "gallery_app rebuild failed after service start or smoke check" \
    "timestamp=${timestamp}; stage=service-start-or-smoke-check-failed-after-rebuild" \
    "rebuild-failed:${timestamp}" \
    "yes" || true

exit 1
