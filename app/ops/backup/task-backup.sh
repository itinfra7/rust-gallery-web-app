#!/bin/sh
set -eu

if [ "$#" -lt 3 ]; then
    echo "usage: $0 BASE_DIR LABEL FILE..." >&2
    exit 64
fi

base_dir=$1
shift
label=$1
shift

timestamp=$(date +%Y%m%d-%H%M%S)
safe_label=$(printf '%s' "$label" | tr -cs 'A-Za-z0-9._-' '-')
backup_dir="${base_dir%/}/${timestamp}-${safe_label}"

mkdir -p "$backup_dir"

for src in "$@"; do
    if [ ! -e "$src" ]; then
        echo "missing: $src" >&2
        exit 66
    fi

    rel_path=$src
    case "$src" in
        /*) rel_path=${src#/} ;;
    esac

    dest_dir="$backup_dir/$(dirname "$rel_path")"
    mkdir -p "$dest_dir"
    cp -Rp "$src" "$dest_dir/"
done

printf '%s\n' "$backup_dir"
