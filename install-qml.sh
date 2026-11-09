#!/usr/bin/env bash
# Copy the widget into the ii Quickshell config and (optionally) patch the bar.
#
#   ./install-qml.sh          copy QML files only
#   ./install-qml.sh --patch  also add the import + ClaudeUsage line to BarContent.qml
#                             (idempotent; backs up BarContent.qml to BarContent.qml.bak first)
#
# Files are copied, not symlinked: Quickshell does not notice edits behind a
# symlinked module directory and keeps serving a stale copy.
set -euo pipefail

II="${II_DIR:-$HOME/.config/quickshell/ii}"
BAR="$II/modules/ii/bar"
SRC="$(cd "$(dirname "$0")" && pwd)/qml/claudeUsage"

[ -d "$BAR" ] || { echo "ii bar dir not found: $BAR" >&2; exit 1; }

mkdir -p "$BAR/claudeUsage"
cp "$SRC"/*.qml "$BAR/claudeUsage/"
echo "copied widget -> $BAR/claudeUsage/"

if [ "${1:-}" = "--patch" ]; then
    f="$BAR/BarContent.qml"
    [ -e "$f.bak" ] || cp "$f" "$f.bak"
    if ! grep -q '^import qs.modules.ii.bar.claudeUsage' "$f"; then
        sed -i '1i import qs.modules.ii.bar.claudeUsage' "$f"
        echo "added import"
    fi
    if ! grep -q 'ClaudeUsage {' "$f"; then
        # Insert right after the BatteryIndicator { ... } block.
        awk '
            /^[[:space:]]*BatteryIndicator[[:space:]]*\{/ { inbat=1; indent=$0; sub(/[^[:space:]].*/, "", indent) }
            { print }
            inbat && $0 ~ "^" indent "}" {
                print ""
                print indent "ClaudeUsage { Layout.alignment: Qt.AlignVCenter }"
                inbat=0
            }
        ' "$f" > "$f.tmp" && mv "$f.tmp" "$f"
        grep -q 'ClaudeUsage {' "$f" && echo "added ClaudeUsage after BatteryIndicator" \
            || echo "WARNING: BatteryIndicator block not found; add the line manually" >&2
    fi
fi

# Quickshell only watches files it loaded; touch the entrypoint to force a reload.
touch "$II/shell.qml"
echo "reloaded quickshell config"
