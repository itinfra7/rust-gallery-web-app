#!/bin/sh

set -eu

ALERT_LIB="${GALLERY_APP_ALERT_LIB:-/usr/local/libexec/gallery-app-alert-lib.sh}"

if [ ! -r "${ALERT_LIB}" ]; then
    echo "Missing alert library: ${ALERT_LIB}" >&2
    exit 1
fi

# shellcheck disable=SC1090
. "${ALERT_LIB}"

message="${1:-Manual gallery-app Telegram alert test from the app server.}"

gallery_app_emit_alert \
    "info" \
    "alert-test" \
    "manual_test" \
    "Manual gallery-app Telegram alert test" \
    "${message}" \
    "alert-test:manual" \
    "yes"

echo "Alert test recorded."
echo "Check ${GALLERY_APP_ALERT_CONFIG:-/usr/local/etc/gallery-app-alert.conf} and ${ALERT_APP_ROOT:-<APP_ROOT>}/run/alerts.ndjson."
