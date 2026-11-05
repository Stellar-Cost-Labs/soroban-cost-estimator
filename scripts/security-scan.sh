#!/usr/bin/env bash
#
# Structural security scanner for soroban-cost-estimator.
#
# This is a *structural* scanner, not a malware engine. It looks for the
# shape of the attack that landed in PR #669: a text payload given a binary
# extension so the GitHub diff renders it as "Binary files differ", editor
# configuration that executes on folder open, and a poisoned .gitignore that
# hides the artifacts.
#
# Usage:
#   scripts/security-scan.sh --diff <base>..<head>     # scan a commit range
#   scripts/security-scan.sh --diff <base>...<head>    # scan a PR range
#   scripts/security-scan.sh --history                 # scan every ref and blob
#
# Options:
#   --allow-review   Downgrade REVIEW findings to notes (exit 0). CI only
#                    passes this when the PR carries the
#                    `security-review-approved` label.
#
# Exit status:
#   0  no BLOCK findings
#   1  at least one BLOCK finding
#   2  usage or environment error
#
# Requires: git, bash 4+, coreutils (od, tr, wc, find). No network access.

set -euo pipefail

SELF="scripts/security-scan.sh"

# Files at or under these paths are high-risk: they make a clone execute code,
# or they can silently change how builds behave.
BLOCK_DIR_TOKENS=(.vscode .devcontainer .idea .cursor .claude .zed .cargo .githooks)
BLOCK_EXACT=(.envrc .mcp.json .cursorrules)

# These are legitimate but must be reviewed by a code owner, not merged by a
# passing test suite alone.
REVIEW_PREFIXES=(.github/ scripts/ build.rs)
REVIEW_EXACT=(Cargo.toml Cargo.lock .gitignore .gitattributes)

# Extensions that are expected to hold binary data. A *text* file here is the
# exact trick used to hide a payload from the web UI diff.
BINARY_EXT_RE='\.(ttf|otf|woff2?|eot|lff|png|jpe?g|gif|ico|bmp|pdf|zip|gz|tgz|bz2|xz|7z|so|dll|dylib|exe|bin|o|a|rlib|rmeta|class|jar|dat|db|sqlite|mp[34]|wav|mov|mp4)$'

# Extensions we expect to be human-readable text.
TEXT_EXT_RE='(\.(md|rs|toml|ya?ml|json|json5|js|mjs|cjs|ts|sh|bash|py|txt|cfg|ini|lock|html|css|scss|proto|snap)$|(^|/)\.(gitignore|gitattributes|editorconfig)$)'

# Download-and-execute and obfuscation markers. Kept in a variable so this
# script's own source can be skipped without disabling the check.
PATTERN_RE='eval\(|new Function\(|atob\(|child_process|require\(.child_process|base64 -d|base64 --decode|curl[^|]*\|[[:space:]]*(ba)?sh|wget[^|]*\|[[:space:]]*(ba)?sh|Invoke-Expression|powershell[^)]*-enc|\\u0065\\u0076\\u0061\\u006C|os\.system\('

# Editor auto-execution markers.
AUTORUN_RE='"runOn"[[:space:]]*:[[:space:]]*"folderOpen"|allowAutomaticTasks'

MODE=""
RANGE=""
ALLOW_REVIEW=0

BLOCKS=0
REVIEWS=0
NOTES=0
FINDINGS=()

TMPDIR_SCAN="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_SCAN"' EXIT

usage() {
    sed -n '3,30p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
}

record() {
    local level="$1" path="$2" message="$3"
    case "$level" in
        BLOCK)
            BLOCKS=$((BLOCKS + 1))
            FINDINGS+=("BLOCK|${path}|${message}")
            ;;
        REVIEW)
            if [ "$ALLOW_REVIEW" -eq 1 ]; then
                NOTES=$((NOTES + 1))
                FINDINGS+=("NOTE|${path}|${message} (code-owner approval recorded)")
            else
                REVIEWS=$((REVIEWS + 1))
                FINDINGS+=("REVIEW|${path}|${message}")
            fi
            ;;
        NOTE)
            NOTES=$((NOTES + 1))
            FINDINGS+=("NOTE|${path}|${message}")
            ;;
    esac
}

# --- path rules -------------------------------------------------------------

check_path() {
    local path="$1" token base
    base="$(basename "$path")"

    for token in "${BLOCK_DIR_TOKENS[@]}"; do
        case "/${path}/" in
            *"/${token}/"*)
                record BLOCK "$path" "touches high-risk editor/build directory '${token}/'"
                return
                ;;
        esac
    done

    for token in "${BLOCK_EXACT[@]}"; do
        if [ "$base" = "$token" ]; then
            record BLOCK "$path" "touches high-risk file '${token}'"
            return
        fi
    done

    for token in "${REVIEW_PREFIXES[@]}"; do
        case "$path" in
            "${token}"*)
                record REVIEW "$path" "changes '${token}' — code-owner review required"
                return
                ;;
        esac
    done

    for token in "${REVIEW_EXACT[@]}"; do
        if [ "$base" = "$token" ]; then
            record REVIEW "$path" "changes '${token}' — code-owner review required"
            return
        fi
    done
}

# is_protected <path> — true when the path sits under a high-risk directory.
is_protected() {
    local path="$1" token
    for token in "${BLOCK_DIR_TOKENS[@]}"; do
        case "/${path}/" in
            *"/${token}/"*) return 0 ;;
        esac
    done
    return 1
}

# --- content rules ----------------------------------------------------------

# inspect_blob <label> <file-on-disk>
inspect_blob() {
    local label="$1" file="$2" size elf nonprint

    [ -f "$file" ] || return 0
    size="$(wc -c <"$file" | tr -d ' ')"
    [ "$size" -eq 0 ] && return 0

    # ELF / PE / Mach-O executables committed as source.
    elf="$(od -An -tx1 -N4 "$file" | tr -d ' \n')"
    case "$elf" in
        7f454c46)
            record BLOCK "$label" "committed ELF executable (${size} bytes)"
            return
            ;;
        4d5a9000)
            record BLOCK "$label" "committed PE executable (${size} bytes)"
            return
            ;;
    esac

    # NUL bytes in something that claims to be text or config.
    if printf '%s' "$label" | grep -Eq "$TEXT_EXT_RE"; then
        if [ "$(tr -d '\0' <"$file" | wc -c | tr -d ' ')" -ne "$size" ]; then
            record BLOCK "$label" "NUL bytes inside a text/config file (${size} bytes) — diff is hidden as binary"
            return
        fi
    fi

    # A text payload wearing a binary extension: strip every printable ASCII
    # byte (tab, LF, CR, 0x20-0x7e); a real font, image, or archive always
    # leaves non-printable bytes behind, a script does not.
    if printf '%s' "$label" | grep -Eq "$BINARY_EXT_RE"; then
        nonprint="$(LC_ALL=C tr -d '\11\12\15\40-\176' <"$file" | wc -c | tr -d ' ')"
        if [ "$nonprint" -eq 0 ]; then
            record BLOCK "$label" "text content in a binary-extension file (${size} bytes) — payload hidden from the diff view"
            return
        fi
    fi

    # Only pattern-match text-ish files; binary noise produces false positives.
    # Markdown is exempt from the marker rules: documentation about the
    # incident necessarily quotes the markers it describes, and a .md file is
    # not executable by this repository's build or tooling. NUL bytes and
    # binary-extension mismatches in markdown are still reported above.
    if ! printf '%s' "$label" | grep -Eq '\.md$' &&
        [ "$(tr -d '\0' <"$file" | wc -c | tr -d ' ')" -eq "$size" ]; then
        if grep -qE "$PATTERN_RE" "$file" 2>/dev/null; then
            record BLOCK "$label" "contains a download/execute or obfuscation marker"
            return
        fi
        if grep -qE "$AUTORUN_RE" "$file" 2>/dev/null; then
            record BLOCK "$label" "configures a task to run automatically on folder open"
            return
        fi
    fi

    # Repository bloat / unvetted build output.
    if [ "$size" -gt 1048576 ]; then
        record REVIEW "$label" "large tracked file (${size} bytes)"
    fi
}

scan_range() {
    local range="$1" status path rev

    rev="${range##*..}"
    rev="${rev#.}"
    [ -n "$rev" ] || rev="HEAD"

    while IFS=$'\t' read -r status path renamed; do
        [ -n "${path:-}" ] || continue

        # Renamed/copied entries come as "R100\told\tnew"; the new path is the
        # one that exists at <head>, so that is what must be inspected.
        if printf '%s' "$status" | grep -qE '^[RC]' && [ -n "${renamed:-}" ]; then
            record NOTE "$renamed" "renamed/copied from '${path}' — review both ends"
            path="$renamed"
        fi

        [ "$path" = "$SELF" ] && continue

        # Deletions add nothing. Removing a protected path is remediation, not
        # an attack, so it is recorded as a note and never blocks a merge.
        if printf '%s' "$status" | grep -q '^D'; then
            if is_protected "$path"; then
                record NOTE "$path" "deleted — removal of a high-risk path"
            fi
            continue
        fi

        check_path "$path"

        local dest="${TMPDIR_SCAN}/blob"
        if git show "${rev}:${path}" >"$dest" 2>/dev/null; then
            inspect_blob "$path" "$dest"
        fi

        # Added lines only, so pre-existing markers are not re-reported.
        # Markdown is exempt for the same reason as above: the security
        # checklist and the incident report necessarily quote what they warn
        # about.
        if ! printf '%s' "$path" | grep -Eq '\.md$' &&
            git diff -U0 "$range" -- "$path" 2>/dev/null | grep -E '^\+' |
                grep -vE '^\+\+\+' | grep -qE "$PATTERN_RE"; then
            record BLOCK "$path" "adds a download/execute or obfuscation marker"
        fi
    done < <(git diff --name-status "$range")
}

scan_history() {
    local token path oid type size dest

    # History mode is an inventory, not a review. "Cargo.toml exists somewhere
    # in this repo's history" is never a finding, so review-only paths are
    # recorded as notes and only structural BLOCKs fail the audit. This is what
    # lets `--history` exit 0 once the payload has actually been purged.
    ALLOW_REVIEW=1

    echo "Scanning every commit and blob reachable from any ref..."

    while IFS= read -r path; do
        [ -n "$path" ] || continue
        [ "$path" = "$SELF" ] && continue
        check_path "$path"
    done < <(git log --all --name-only --pretty=format: | sort -u)

    while IFS=' ' read -r oid path; do
        [ -n "${path:-}" ] || continue
        [ "$path" = "$SELF" ] && continue

        type="$(git cat-file -t "$oid" 2>/dev/null || echo blob)"
        [ "$type" = "blob" ] || continue

        printf '%s' "$path" | grep -Eq "$BINARY_EXT_RE" || continue

        size="$(git cat-file -s "$oid" 2>/dev/null || echo 0)"
        # Skip absurdly large blobs; the extension check above already caught
        # the interesting case and a 100 MB object proves nothing new.
        [ "$size" -gt 5000000 ] && continue

        dest="${TMPDIR_SCAN}/history-blob"
        if git cat-file -p "$oid" >"$dest" 2>/dev/null; then
            inspect_blob "$path" "$dest"
        fi
    done < <(git rev-list --objects --all | awk 'NF > 1 { print $1, $2 }')

    for token in .vscode public/fonts test_comfy target2; do
        if git rev-list --objects --all | grep -q " ${token}"; then
            record NOTE "" "history still contains '${token}' objects (a rewrite is required to purge them)"
        fi
    done
}

# --- main -------------------------------------------------------------------

while [ $# -gt 0 ]; do
    case "$1" in
        --diff)
            MODE="diff"
            RANGE="${2:-}"
            [ -n "$RANGE" ] || usage
            shift 2
            ;;
        --history)
            MODE="history"
            shift
            ;;
        --allow-review)
            ALLOW_REVIEW=1
            shift
            ;;
        -h | --help)
            usage
            ;;
        *)
            echo "unknown argument: $1" >&2
            usage
            ;;
    esac
done

[ -n "$MODE" ] || usage

git rev-parse --git-dir >/dev/null 2>&1 || {
    echo "not a git repository" >&2
    exit 2
}

case "$MODE" in
    diff)
        range_base="${RANGE%%..*}"
        range_head="${RANGE#*..}"
        range_head="${range_head#.}"
        for spec in "$range_base" "${range_head:-HEAD}"; do
            if [ -z "$spec" ] || ! git rev-parse --verify --quiet "${spec}^{commit}" >/dev/null; then
                echo "cannot resolve range endpoint '${spec}' in '${RANGE}'" >&2
                exit 2
            fi
        done
        echo "### Structural security scan"
        echo
        echo "Range: \`$RANGE\`"
        scan_range "$RANGE"
        ;;
    history)
        echo "### Structural security scan (full history)"
        scan_history
        ;;
esac

echo
if [ "${#FINDINGS[@]}" -eq 0 ]; then
    echo "No findings. Nothing in this change has the shape of a payload."
    exit 0
fi

for level in BLOCK REVIEW NOTE; do
    for finding in "${FINDINGS[@]}"; do
        [ "${finding%%|*}" = "$level" ] || continue
        rest="${finding#*|}"
        path="${rest%%|*}"
        message="${rest#*|}"
        echo "- **${level}** \`${path:-<history>}\` — ${message}"
    done
done

echo
echo "Summary: ${BLOCKS} blocking, ${REVIEWS} review-required, ${NOTES} notes."

if [ "$BLOCKS" -gt 0 ]; then
    echo
    echo "Failing: this change contains a blocking finding. See SECURITY.md."
    exit 1
fi

if [ "$REVIEWS" -gt 0 ]; then
    echo
    echo "Failing: this change touches protected paths and needs code-owner approval."
    exit 1
fi

exit 0
