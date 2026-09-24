#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
spec_file="$repo_root/packaging/yadal-git.spec"
version="0.3.0^git$(git -C "$repo_root" rev-parse --short HEAD)"
temporary_spec="$(mktemp --suffix=.spec)"

cleanup() {
    rm -f "$temporary_spec"
}
trap cleanup EXIT

for command in git spectool rpmbuild; do
    if ! command -v "$command" >/dev/null 2>&1; then
        printf 'error: required command not found: %s\n' "$command" >&2
        exit 1
    fi
done

awk -v version="$version" '
    /^%global version / {
        print "%global version " version
        next
    }
    { print }
' "$spec_file" >"$temporary_spec"

spectool -g -R --force "$temporary_spec"
rpmbuild -ba "$temporary_spec"
