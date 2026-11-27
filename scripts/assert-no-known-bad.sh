#!/usr/bin/env bash
#
# Known-bad guard for soroban-cost-estimator.
#
# Fails if material from the 2026-10-29 supply-chain incident
# (docs/INCIDENT-REPORT.md) is reachable from ANY ref, or if a removed path
# exists in the index. This is a *regression* check: history was rewritten to
# remove these objects, and nothing may ever put them back — not a merged PR, not
# a stray branch, not a restored backup pushed by accident.
#
# The banned IDs and paths are embedded here rather than in a data file so that
# there is exactly one artifact to trust. CI runs THIS FILE FROM THE BASE BRANCH,
# so a pull request cannot widen the allowlist.
#
# Usage:
#   scripts/assert-no-known-bad.sh
#
# Exit status:
#   0  nothing known-bad is reachable
#   1  the payload blob or a removed path is reachable (or staged)
#   2  usage or environment error
#
# Requires: git, bash 4+, coreutils. No network access.

set -euo pipefail

# --- never change these without a documented reason ------------------------
# Blob IDs recorded in docs/INCIDENT-REPORT.md. Removing an entry weakens the
# guard permanently, so entries are append-only.

KNOWN_BAD_BLOBS=(
    # public/fonts/fa-regular.lff and public/fonts/fa-solid-300.ttf — the two
    # byte-identical 12,681-byte obfuscated JavaScript payloads.
    38ba99d7bb27fdeb45b82c6fdffe8ecca03108bb
    # .gitignore rewritten with embedded NUL bytes so its diff rendered as
    # "Binary files differ" and could not be reviewed.
    aa84bf7c7e7783568d3a564a90ea696a85d73898
)

# Paths removed from all history. Trailing slash = directory.
REMOVED_PATHS=(
    .vscode/
    public/
    test_comfy/
    target2/
)

# ---------------------------------------------------------------------------

fail=0

note_fail() {
    fail=1
    printf '%s\n' "$1"
}

git rev-parse --git-dir >/dev/null 2>&1 || {
    echo "not a git repository" >&2
    exit 2
}

echo "### Known-bad guard"
echo

# --- 1. banned blobs reachable from any ref --------------------------------
objects="$(git rev-list --objects --all)"

for blob in "${KNOWN_BAD_BLOBS[@]}"; do
    line="$(printf '%s\n' "$objects" | grep -m1 "^${blob}" || true)"
    if [ -z "$line" ]; then
        printf 'ok    blob %s… absent from every ref\n' "${blob:0:10}"
        continue
    fi

    path="${line#"$blob"}"
    path="${path# }"
    note_fail "FAIL  blob ${blob} is reachable (${path:-<no path>})"
    echo "      reintroduced by:"
    git log --all --oneline --format='        %h %ad %s' --date=short -- "$path" 2>/dev/null |
        head -5 || true
done

# --- 2. removed paths in any commit on any ref -----------------------------
all_paths="$(git log --all --name-only --pretty=format: | LC_ALL=C sort -u)"

for prefix in "${REMOVED_PATHS[@]}"; do
    hits="$(printf '%s\n' "$all_paths" | grep -E "^${prefix//./\\.}" || true)"
    if [ -z "$hits" ]; then
        printf 'ok    path %s absent from every ref\n' "$prefix"
        continue
    fi

    note_fail "FAIL  removed path '${prefix}' is present on at least one ref"
    printf '%s\n' "$hits" | head -10 | sed 's/^/        /'
    count="$(printf '%s\n' "$hits" | wc -l | tr -d ' ')"
    if [ "$count" -gt 10 ]; then
        echo "        … and $((count - 10)) more file(s)"
    fi
done

# --- 3. the same paths staged locally -------------------------------------
indexed="$(git ls-files)"
for prefix in "${REMOVED_PATHS[@]}"; do
    if printf '%s\n' "$indexed" | grep -qE "^${prefix//./\\.}"; then
        note_fail "FAIL  removed path '${prefix}' is staged in the index"
    fi
done

echo
if [ "$fail" -ne 0 ]; then
    echo "Failing: known-bad material is reachable. See docs/INCIDENT-REPORT.md"
    echo "and docs/MAINTAINER-CHECKLIST.md. Do not merge, do not push."
    exit 1
fi

echo "Clean: no known-bad blob and no removed path is reachable from any ref."
exit 0
