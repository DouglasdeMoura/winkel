#!/usr/bin/env bash
# Retakes docs/screenshots: three views in each of the two themes the README
# shows. Needs a Wayland session with grim and Hyprland (or another
# compositor that maps class dev.douglasmoura.winkel).
#
#   tools/screenshots.sh              # into docs/screenshots
#   tools/screenshots.sh /tmp/shots   # somewhere else, to compare first
#
# Each theme is shot under a HOME of its own, because the theme module reads
# the live theme out of $HOME/.local/state/omarchy/current.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${1:-$root/docs/screenshots}"
bin="$root/target/release/winkel"
themes=(tokyo-night catppuccin-latte)

[ -x "$bin" ] || { echo "build first: cargo build --release" >&2; exit 1; }
command -v grim >/dev/null || { echo "grim is required" >&2; exit 1; }
command -v hyprctl >/dev/null || { echo "hyprctl is required" >&2; exit 1; }

installed="$HOME/.local/share/omarchy/themes"
shell_toml="$HOME/.local/state/omarchy/current/theme/shell.toml"
for theme in "${themes[@]}"; do
    [ -f "$installed/$theme/colors.toml" ] || { echo "no omarchy theme '$theme' installed" >&2; exit 1; }
done
[ -f "$shell_toml" ] || { echo "no omarchy shell.toml at $shell_toml" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"; pkill -x winkel 2>/dev/null || true' EXIT
mkdir -p "$out"
monitor=$(hyprctl -j monitors | jq -r '.[0].name')

kill_app() {
    pkill -x winkel 2>/dev/null || true
    pkill -f '[w]inkel --backend' 2>/dev/null || true
    sleep 0.5
}

shot_view() {
    local theme="$1" view="$2" home="$3"
    kill_app
    HOME="$home" WINKEL_SILENT=1 WINKEL_UI="$root/ui" setsid nohup "$bin" >/dev/null 2>&1 </dev/null &
    local tries=0
    until hyprctl -j clients | jq -e '.[] | select(.class == "dev.douglasmoura.winkel")' >/dev/null 2>&1; do
        sleep 0.3
        tries=$((tries + 1))
        [ "$tries" -gt 40 ] && { echo "$theme $view did not map" >&2; exit 1; }
    done
    sleep 1.5
    case "$view" in
        main) ;;
        time-signature)
            # Focus and open the time-signature dialog (click the signature).
            wtype -k tab -k tab -k tab -k tab -k tab -k return 2>/dev/null || true
            sleep 0.8
            ;;
        subdivision)
            wtype -k tab -k tab -k tab -k tab -k tab -k tab -k return 2>/dev/null || true
            sleep 0.8
            ;;
    esac
    local rect
    rect=$(hyprctl -j clients | jq -r --argjson mx "$(hyprctl -j monitors | jq -r --arg n "$monitor" '.[] | select(.name == $n) | .x')" --argjson my "$(hyprctl -j monitors | jq -r --arg n "$monitor" '.[] | select(.name == $n) | .y')" \
        '[.[] | select(.class == "dev.douglasmoura.winkel")][0] | "\(.size[0])x\(.size[1])+\(.at[0] - $mx)+\(.at[1] - $my)"')
    grim -o "$monitor" "$work/full.png"
    magick "$work/full.png" -crop "$rect" +repage "$out/$theme-$view.png"
    echo "$out/$theme-$view.png"
}

for theme in "${themes[@]}"; do
    home="$work/$theme"
    mkdir -p "$home/.local/state/omarchy/current/theme" "$home/runtime"
    cp "$installed/$theme/colors.toml" "$home/.local/state/omarchy/current/theme/"
    cp "$shell_toml" "$home/.local/state/omarchy/current/theme/"
    printf '%s' "$theme" > "$home/.local/state/omarchy/current/theme.name"
    export XDG_RUNTIME_DIR="$home/runtime"
    for view in main time-signature subdivision; do
        shot_view "$theme" "$view" "$home"
    done
done
kill_app
