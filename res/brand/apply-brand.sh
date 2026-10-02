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
# Besides artwork, the folder's name goes into the title bar, an optional site.txt
# at its root sets the "Website" links and an optional cor.txt the app's colours;
# all of it lands in flutter/lib/brand.dart.
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

BRAND_DART="$REPO_ROOT/flutter/lib/brand.dart"
BRAND_RS="$REPO_ROOT/src/brand.rs"
SITE_FILE="site.txt"
COLOR_FILE="cor.txt"
COLORS_PY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/brand_colors.py"

# Documentation that travels with a brand folder but is not part of the product.
is_doc() {
    case "$1" in
        LEIAME.md|README.md|README*.txt|.gitattributes) return 0 ;;
        *) return 1 ;;
    esac
}

# --- What the brand changes besides artwork (flutter/lib/brand.dart) ----------
#
# The folder name labels the title bar ("BR Remote - Invicta"), site.txt is where
# the "Website" links go and cor.txt is the brand's main colour, each file only if
# the folder has one. All are validated before anything is copied: a bad value must
# stop the build, not ship a broken link or an unreadable button.

# A one-value text file as a person saves it: drop a BOM and CRs (Notepad, CRLF
# checkouts), surrounding spaces and blank lines.
read_value_file() {
    sed '1s/^\xEF\xBB\xBF//' "$1" | tr -d '\r' \
        | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' | grep -v '^$' || true
}

brand_name="$(basename "$BRAND_DIR")"
case "$brand_name" in
    # The committed artwork, i.e. the default build: no suffix in the title.
    brremote) brand_name="" ;;
esac
name_re='^[A-Za-z0-9 _.-]*$'
if ! [[ "$brand_name" =~ $name_re ]]; then
    echo "apply-brand: brand folder name has characters the app cannot show: $brand_name" >&2
    exit 1
fi

brand_site=""
if [ -f "$BRAND_DIR/$SITE_FILE" ]; then
    # Without scheme and trailing slash, since the app adds https:// itself.
    brand_site="$(read_value_file "$BRAND_DIR/$SITE_FILE")"
    brand_site="${brand_site#http://}"
    brand_site="${brand_site#https://}"
    brand_site="${brand_site%/}"
    # [[ =~ ]] and not grep: grep would pass a file with a second, bad line.
    site_re='^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+(/[A-Za-z0-9._~/-]*)?$'
    if ! [[ "$brand_site" =~ $site_re ]]; then
        echo "apply-brand: $SITE_FILE must hold one site, like www.example.com.br; found:" >&2
        sed 's/^/  | /' "$BRAND_DIR/$SITE_FILE" >&2
        exit 1
    fi
fi

brand_color=""
brand_palette=""
if [ -f "$BRAND_DIR/$COLOR_FILE" ]; then
    brand_color="$(read_value_file "$BRAND_DIR/$COLOR_FILE")"
    brand_color="${brand_color#\#}"
    color_re='^[0-9A-Fa-f]{6}$'
    if ! [[ "$brand_color" =~ $color_re ]]; then
        echo "apply-brand: $COLOR_FILE must hold one colour, like #0070C8; found:" >&2
        sed 's/^/  | /' "$BRAND_DIR/$COLOR_FILE" >&2
        exit 1
    fi
    brand_color="#$(printf '%s' "$brand_color" | tr 'a-f' 'A-F')"

    # The other tones are derived (brand_colors.py says how), with whichever Python
    # runs here: on Windows "python3" can be a Store stub that runs nothing.
    py=""
    for candidate in python3 python; do
        if "$candidate" -c 'import colorsys' >/dev/null 2>&1; then
            py="$candidate"
            break
        fi
    done
    if [ -z "$py" ]; then
        echo "apply-brand: $COLOR_FILE needs Python to derive the palette, and none runs here" >&2
        exit 1
    fi
    # tr: Python on Windows ends its lines in CRLF.
    if ! brand_palette="$("$py" "$COLORS_PY" "$brand_color" | tr -d '\r')"; then
        echo "apply-brand: brand_colors.py failed for $brand_color" >&2
        exit 1
    fi
    palette_line='kBrand[A-Za-z]+=0xFF[0-9A-F]{6}'
    palette_re="^($palette_line"$'\n'"){4}$palette_line\$"
    if ! [[ "$brand_palette" =~ $palette_re ]]; then
        echo "apply-brand: brand_colors.py gave an unexpected palette:" >&2
        printf '%s\n' "$brand_palette" | sed 's/^/  | /' >&2
        exit 1
    fi
fi

# --- Collect and validate everything before copying a single byte ------------

assets=()
errors=()

while IFS= read -r abs; do
    rel="${abs#$BRAND_DIR/}"
    is_doc "$rel" && continue
    [ "$rel" = "$SITE_FILE" ] && continue   # data, read above - not artwork
    [ "$rel" = "$COLOR_FILE" ] && continue  # same
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

# Patch brand.dart line by line, so its comments and the default site stay in one
# place. The patterns stop at the closing quote, leaving a CR from a CRLF checkout
# alone, and each result is checked: a pattern that matched nothing fails here.
set_brand_const() {
    local name="$1" value="$2"
    sed -i "s|^const String $name = '[^']*';|const String $name = '$value';|" "$BRAND_DART"
    if ! grep -Fq "const String $name = '$value';" "$BRAND_DART"; then
        echo "apply-brand: could not set $name in $BRAND_DART" >&2
        exit 1
    fi
}

# Same, for the 0xAARRGGBB colour constants; their trailing comment survives.
set_brand_int() {
    local name="$1" value="$2"
    sed -i "s|^const int $name = 0x[0-9A-Fa-f]\{8\};|const int $name = $value;|" "$BRAND_DART"
    if ! grep -Fq "const int $name = $value;" "$BRAND_DART"; then
        echo "apply-brand: could not set $name in $BRAND_DART" >&2
        exit 1
    fi
}

set_brand_const kBrandFolder "$brand_name"
echo "  title     ${brand_name:-(no suffix)}"

# The core's copy of the brand (src/brand.rs): the machine registers itself in this
# brand's catalog. The "&" in the replacement is escaped - unescaped, sed would put
# the whole matched line there.
sed -i "s|^pub const BRAND_FOLDER: &str = \"[^\"]*\";|pub const BRAND_FOLDER: \&str = \"$brand_name\";|" "$BRAND_RS"
if ! grep -Fq "pub const BRAND_FOLDER: &str = \"$brand_name\";" "$BRAND_RS"; then
    echo "apply-brand: could not set BRAND_FOLDER in $BRAND_RS" >&2
    exit 1
fi
echo "  catalog   ${brand_name:-(default)}"
if [ -n "$brand_site" ]; then
    set_brand_const kBrandWebsite "$brand_site"
    echo "  website   $brand_site"
else
    echo "  website   (no $SITE_FILE in the brand folder, keeping the default)"
fi
if [ -n "$brand_palette" ]; then
    echo "  color     $brand_color"
    while IFS='=' read -r name value; do
        set_brand_int "$name" "$value"
        echo "              $name = $value"
    done <<< "$brand_palette"
else
    echo "  color     (no $COLOR_FILE in the brand folder, keeping the default)"
fi

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
    # tr strips the CR that a CRLF checkout (Windows runners) would leave on
    # every line, which would otherwise make each path look uncovered.
    done < <(tr -d '\r' < "$KNOWN_PATHS")

    if [ "${#missing[@]}" -gt 0 ]; then
        echo "apply-brand: not overridden by this brand (keeping what is committed):"
        for rel in "${missing[@]}"; do
            echo "  kept      $rel"
        done
    fi
fi
