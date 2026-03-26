#!/bin/sh

set -eu

: "${GALLERY_WEB_WATCHDOG_CONFIG:=/usr/local/etc/gallery-app-web-watchdog.conf}"

watchdog_preserve_env_var() {
    name="${1}"
    eval "case \${${name}+x} in x) eval \"__WATCHDOG_ENV_${name}=\\\${${name}}\" ;; esac"
}

watchdog_restore_env_var() {
    name="${1}"
    eval "case \${__WATCHDOG_ENV_${name}+x} in x) ${name}=\${__WATCHDOG_ENV_${name}} ;; esac"
}

for watchdog_var in \
    WATCHDOG_TARGET_URL \
    WATCHDOG_TARGET_NAME \
    WATCHDOG_HTTP_TIMEOUT_SECONDS \
    WATCHDOG_LOOP_SLEEP_SECONDS \
    WATCHDOG_STATE_DIR \
    WATCHDOG_STATE_FILE \
    WATCHDOG_WEB_ZT_IP \
    WATCHDOG_FAILURE_KIND \
    WATCHDOG_RECOVERY_KIND \
    WATCHDOG_TEST_FAILURE_KIND \
    WATCHDOG_TEST_RECOVERY_KIND \
    WATCHDOG_TEST_MODE \
    WATCHDOG_RUN_ONCE \
    WATCHDOG_CURL_BIN \
    GALLERY_APP_ALERT_CONFIG
do
    watchdog_preserve_env_var "${watchdog_var}"
done

if [ -r "${GALLERY_WEB_WATCHDOG_CONFIG}" ]; then
    # shellcheck disable=SC1090
    . "${GALLERY_WEB_WATCHDOG_CONFIG}"
fi

for watchdog_var in \
    WATCHDOG_TARGET_URL \
    WATCHDOG_TARGET_NAME \
    WATCHDOG_HTTP_TIMEOUT_SECONDS \
    WATCHDOG_LOOP_SLEEP_SECONDS \
    WATCHDOG_STATE_DIR \
    WATCHDOG_STATE_FILE \
    WATCHDOG_WEB_ZT_IP \
    WATCHDOG_FAILURE_KIND \
    WATCHDOG_RECOVERY_KIND \
    WATCHDOG_TEST_FAILURE_KIND \
    WATCHDOG_TEST_RECOVERY_KIND \
    WATCHDOG_TEST_MODE \
    WATCHDOG_RUN_ONCE \
    WATCHDOG_CURL_BIN \
    GALLERY_APP_ALERT_CONFIG
do
    watchdog_restore_env_var "${watchdog_var}"
done

: "${WATCHDOG_TARGET_URL:=https://<PUBLIC_DOMAIN>/}"
: "${WATCHDOG_TARGET_NAME:=gallery-web-public}"
: "${WATCHDOG_HTTP_TIMEOUT_SECONDS:=10}"
: "${WATCHDOG_LOOP_SLEEP_SECONDS:=5}"
: "${WATCHDOG_STATE_DIR:=/var/db/<APP_SLUG>-web-watchdog/state}"
: "${WATCHDOG_STATE_FILE:=${WATCHDOG_STATE_DIR}/connectivity.state}"
: "${WATCHDOG_WEB_ZT_IP:=<WEB_ZEROTIER_IP>}"
: "${WATCHDOG_FAILURE_KIND:=app_web_public_down}"
: "${WATCHDOG_RECOVERY_KIND:=app_web_public_recovered}"
: "${WATCHDOG_TEST_FAILURE_KIND:=app_web_public_down_test}"
: "${WATCHDOG_TEST_RECOVERY_KIND:=app_web_public_recovered_test}"
: "${WATCHDOG_TEST_MODE:=no}"
: "${WATCHDOG_RUN_ONCE:=no}"
: "${WATCHDOG_CURL_BIN:=/usr/local/bin/curl}"
: "${GALLERY_APP_ALERT_CONFIG:=/usr/local/etc/gallery-app-alert.conf}"

if [ ! -x "${WATCHDOG_CURL_BIN}" ]; then
    WATCHDOG_CURL_BIN=$(command -v curl 2>/dev/null || printf 'curl')
fi

unset watchdog_var

# shellcheck disable=SC1091
. /usr/local/libexec/gallery-app-alert-lib.sh

install -d -o root -g wheel -m 755 "${WATCHDOG_STATE_DIR}"

watchdog_read_state() {
    if [ -r "${WATCHDOG_STATE_FILE}" ]; then
        state=$(head -n 1 "${WATCHDOG_STATE_FILE}" 2>/dev/null || printf 'unknown')
        case "${state}" in
            up|down)
                printf '%s\n' "${state}"
                return 0
                ;;
        esac
    fi

    printf 'unknown\n'
}

watchdog_write_state() {
    printf '%s\n' "${1}" > "${WATCHDOG_STATE_FILE}"
    chown root:wheel "${WATCHDOG_STATE_FILE}"
    chmod 640 "${WATCHDOG_STATE_FILE}"
}

watchdog_check_target() {
    "${WATCHDOG_CURL_BIN}" -fsS -L --max-time "${WATCHDOG_HTTP_TIMEOUT_SECONDS}" -o /dev/null "${WATCHDOG_TARGET_URL}" >/dev/null 2>&1
}

watchdog_probe_zt_ping() {
    if [ -z "${WATCHDOG_WEB_ZT_IP}" ]; then
        printf 'unknown\n'
        return 0
    fi

    if ping -c 1 -W 1 "${WATCHDOG_WEB_ZT_IP}" >/dev/null 2>&1; then
        printf 'up\n'
    else
        printf 'down\n'
    fi
}

watchdog_emit_down() {
    zt_ping_state="${1}"
    kind="${WATCHDOG_FAILURE_KIND}"
    summary="App lost public HTTPS reachability to web"

    if [ "${WATCHDOG_TEST_MODE}" = "yes" ]; then
        kind="${WATCHDOG_TEST_FAILURE_KIND}"
        summary="TEST: App lost public HTTPS reachability to web"
    fi

    gallery_app_emit_alert \
        critical \
        watchdog \
        "${kind}" \
        "${summary}" \
        "target_url=${WATCHDOG_TARGET_URL}; target_name=${WATCHDOG_TARGET_NAME}; http_timeout=${WATCHDOG_HTTP_TIMEOUT_SECONDS}; web_zt_ip=${WATCHDOG_WEB_ZT_IP}; web_zt_ping=${zt_ping_state}" \
        "watchdog:${WATCHDOG_TARGET_URL}:down"
}

watchdog_emit_recovery() {
    zt_ping_state="${1}"
    kind="${WATCHDOG_RECOVERY_KIND}"
    summary="App regained public HTTPS reachability to web"

    if [ "${WATCHDOG_TEST_MODE}" = "yes" ]; then
        kind="${WATCHDOG_TEST_RECOVERY_KIND}"
        summary="TEST: App regained public HTTPS reachability to web"
    fi

    gallery_app_emit_alert \
        info \
        watchdog \
        "${kind}" \
        "${summary}" \
        "target_url=${WATCHDOG_TARGET_URL}; target_name=${WATCHDOG_TARGET_NAME}; http_timeout=${WATCHDOG_HTTP_TIMEOUT_SECONDS}; web_zt_ip=${WATCHDOG_WEB_ZT_IP}; web_zt_ping=${zt_ping_state}" \
        "watchdog:${WATCHDOG_TARGET_URL}:up"
}

watchdog_run_cycle() {
    previous_state=$(watchdog_read_state)
    current_state=down
    zt_ping_state=unknown

    if watchdog_check_target; then
        current_state=up
    fi

    zt_ping_state=$(watchdog_probe_zt_ping)

    if [ "${current_state}" = "down" ] && [ "${previous_state}" != "down" ]; then
        watchdog_emit_down "${zt_ping_state}"
    fi

    if [ "${current_state}" = "up" ] && [ "${previous_state}" = "down" ]; then
        watchdog_emit_recovery "${zt_ping_state}"
    fi

    watchdog_write_state "${current_state}"
}

while :; do
    watchdog_run_cycle

    if [ "${WATCHDOG_RUN_ONCE}" = "yes" ]; then
        exit 0
    fi

    sleep "${WATCHDOG_LOOP_SLEEP_SECONDS}"
done
