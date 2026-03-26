#!/bin/sh

set -eu

: "${GALLERY_APP_WATCHDOG_CONFIG:=/usr/local/etc/gallery-app-watchdog.conf}"

watchdog_preserve_env_var() {
    name="${1}"
    eval "case \${${name}+x} in x) eval \"__WATCHDOG_ENV_${name}=\\\${${name}}\" ;; esac"
}

watchdog_restore_env_var() {
    name="${1}"
    eval "case \${__WATCHDOG_ENV_${name}+x} in x) ${name}=\${__WATCHDOG_ENV_${name}} ;; esac"
}

for watchdog_var in \
    WATCHDOG_TARGET_IP \
    WATCHDOG_TARGET_NAME \
    WATCHDOG_PING_COUNT \
    WATCHDOG_PING_TIMEOUT_SECONDS \
    WATCHDOG_LOOP_SLEEP_SECONDS \
    WATCHDOG_STATE_DIR \
    WATCHDOG_STATE_FILE \
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

if [ -r "${GALLERY_APP_WATCHDOG_CONFIG}" ]; then
    # shellcheck disable=SC1090
    . "${GALLERY_APP_WATCHDOG_CONFIG}"
fi

for watchdog_var in \
    WATCHDOG_TARGET_IP \
    WATCHDOG_TARGET_NAME \
    WATCHDOG_PING_COUNT \
    WATCHDOG_PING_TIMEOUT_SECONDS \
    WATCHDOG_LOOP_SLEEP_SECONDS \
    WATCHDOG_STATE_DIR \
    WATCHDOG_STATE_FILE \
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

: "${WATCHDOG_TARGET_IP:=<APP_ZEROTIER_IP>}"
: "${WATCHDOG_TARGET_NAME:=gallery-app}"
: "${WATCHDOG_PING_COUNT:=10}"
: "${WATCHDOG_PING_TIMEOUT_SECONDS:=1}"
: "${WATCHDOG_LOOP_SLEEP_SECONDS:=5}"
: "${WATCHDOG_STATE_DIR:=/var/db/<APP_SLUG>-watchdog/state}"
: "${WATCHDOG_STATE_FILE:=${WATCHDOG_STATE_DIR}/connectivity.state}"
: "${WATCHDOG_FAILURE_KIND:=web_app_ping_down}"
: "${WATCHDOG_RECOVERY_KIND:=web_app_ping_recovered}"
: "${WATCHDOG_TEST_FAILURE_KIND:=web_app_ping_down_test}"
: "${WATCHDOG_TEST_RECOVERY_KIND:=web_app_ping_recovered_test}"
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
    ping -c "${WATCHDOG_PING_COUNT}" -W "${WATCHDOG_PING_TIMEOUT_SECONDS}" "${WATCHDOG_TARGET_IP}" >/dev/null 2>&1
}

watchdog_emit_down() {
    kind="${WATCHDOG_FAILURE_KIND}"
    summary="Web lost ZeroTier ICMP reachability to app for 10 seconds"

    if [ "${WATCHDOG_TEST_MODE}" = "yes" ]; then
        kind="${WATCHDOG_TEST_FAILURE_KIND}"
        summary="TEST: Web lost ZeroTier ICMP reachability to app for 10 seconds"
    fi

    gallery_app_emit_alert \
        critical \
        watchdog \
        "${kind}" \
        "${summary}" \
        "target_ip=${WATCHDOG_TARGET_IP}; target_name=${WATCHDOG_TARGET_NAME}; ping_count=${WATCHDOG_PING_COUNT}; ping_timeout=${WATCHDOG_PING_TIMEOUT_SECONDS}" \
        "watchdog:${WATCHDOG_TARGET_IP}:down"
}

watchdog_emit_recovery() {
    kind="${WATCHDOG_RECOVERY_KIND}"
    summary="Web regained ZeroTier ICMP reachability to app"

    if [ "${WATCHDOG_TEST_MODE}" = "yes" ]; then
        kind="${WATCHDOG_TEST_RECOVERY_KIND}"
        summary="TEST: Web regained ZeroTier ICMP reachability to app"
    fi

    gallery_app_emit_alert \
        info \
        watchdog \
        "${kind}" \
        "${summary}" \
        "target_ip=${WATCHDOG_TARGET_IP}; target_name=${WATCHDOG_TARGET_NAME}; ping_count=${WATCHDOG_PING_COUNT}; ping_timeout=${WATCHDOG_PING_TIMEOUT_SECONDS}" \
        "watchdog:${WATCHDOG_TARGET_IP}:up"
}

watchdog_run_cycle() {
    previous_state=$(watchdog_read_state)
    current_state=down

    if watchdog_check_target; then
        current_state=up
    fi

    if [ "${current_state}" = "down" ] && [ "${previous_state}" != "down" ]; then
        watchdog_emit_down
    fi

    if [ "${current_state}" = "up" ] && [ "${previous_state}" = "down" ]; then
        watchdog_emit_recovery
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
