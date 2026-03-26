#!/bin/sh

set -eu

APP_ROOT="<APP_ROOT>"
APP_BINARY="${APP_ROOT}/bin/<APP_SLUG>"

export HOME="/home/<APP_USER>"
export USER="<APP_USER>"
export LOGNAME="<APP_USER>"
export PATH="/usr/local/bin:/usr/bin:/bin"

cd "${APP_ROOT}"
exec "${APP_BINARY}"
