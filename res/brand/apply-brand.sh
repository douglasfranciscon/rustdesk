#!/usr/bin/env bash
#
# CUSTOM BRANDING: swap the product's icons/logos for another brand's before a build.
#
# A brand folder mirrors this repository's own layout, so the folder itself is the
# mapping - no list to keep in sync here. Example:
#
#     <brand>/res/icon.ico
#     <brand>/flutter/assets/logo.png
#     <brand>/flutter/android/app/src/main/res/mipmap-xxxhdpi/ic_launcher.png
#
# Usage: bash res/brand/apply-brand.sh <brand-folder> [repo-root]
#
# Every file in the brand folder MUST land on a path that already exists, otherwise
# it is a typo: the file would be copied somewhere nothing reads, and the build would
# quietly ship the old artwork. That is an error, not a warning.
#
# See docs/1_MarcasAlternativas.md.

set -euo pipefail

BRAND_DIR="${1:-}"
REPO_ROOT="${2:-}"

if [ -z "$BRAND_DIR" ]; then
    echo "usage: bash res/brand/apply-brand.sh <brand-folder> [repo-root]" >&2
    exit 2
fi

if [ -z "$REPO_ROOT" ]; then
    # Default to the repository this script lives in (res/brand/ -> ../..).
    REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fi

if [ ! -d "$BRAND_DIR" ]; then
    echo "apply-brand: brand folder not found: $BRAND_DIR" >&2
    exit 1
fi
if [ ! -d "$REPO_ROOT" ]; then
    echo "apply-brand: repository root not found: $REPO_ROOT" >&2
    exit 1
fi

BRAND_DIR="$(cd "$BRAND_DIR" && pwd)"
KNOWN_PATHS="$REPO_ROOT/res/brand/paths.txt"

# Documentation that travels with a brand folder but is not part of the product.
is_doc() {
    case "$1" in
        LEIAME.md|README.md|README*.txt|.gitattributes) return 0 ;;
        *) return 1 ;;
    esac
}

# --- Collect and validate everything before copying a single byte ------------

assets=()
errors=()

while IFS= read -r abs; do
    rel="${abs#$BRAND_DIR/}"
    is_doc "$rel" && continue
    if [ -f "$REPO_ROOT/$rel" ]; then
        assets+=("$rel")
    else
        errors+=("$rel")
    fi
done < <(find "$BRAND_DIR" -type f | sort)

if [ "${#errors[@]}" -gt 0 ]; then
    echo "apply-brand: these files have no matching path in the repository:" >&2
    for rel in "${errors[@]}"; do
        echo "  $rel" >&2
    done
    echo "apply-brand: fix the paths in the brand folder (they mirror the repository)." >&2
    exit 1
fi

if [ "${#assets[@]}" -eq 0 ]; then
    echo "apply-brand: brand folder has no assets: $BRAND_DIR" >&2
    exit 1
fi

# --- Apply ------------------------------------------------------------------

for rel in "${assets[@]}"; do
    cp -f "$BRAND_DIR/$rel" "$REPO_ROOT/$rel"
    echo "  replaced  $rel"
done
echo "apply-brand: ${#assets[@]} file(s) replaced from $BRAND_DIR"

# --- Remind about the branding paths this brand did not cover ----------------

if [ -f "$KNOWN_PATHS" ]; then
    missing=()
    while IFS= read -r known; do
        case "$known" in ''|'#'*) continue ;; esac
        covered=0
        for rel in "${assets[@]}"; do
            [ "$rel" = "$known" ] && { covered=1; break; }
        done
        [ "$covered" -eq 0 ] && missing+=("$known")
    done < "$KNOWN_PATHS"

    if [ "${#missing[@]}" -gt 0 ]; then
        echo "apply-brand: not overridden by this brand (keeping what is committed):"
        for rel in "${missing[@]}"; do
            echo "  kept      $rel"
        done
    fi
fi
