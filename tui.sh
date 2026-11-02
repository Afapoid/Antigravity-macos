#!/bin/sh
# curl -fsSL https://raw.githubusercontent.com/Afapoid/Antigravity-macos/main/tui.sh | sh
set -eu
REPO="${AG_REPO:-Afapoid/Antigravity-macos}"
case "$(uname -s)" in
    Darwin)
        ARCH=$(uname -m)
        if [ "$ARCH" = arm64 ] || [ "$ARCH" = aarch64 ] ||
            [ "$(sysctl -n hw.optional.arm64 2>/dev/null || true)" = 1 ]; then
            PLATFORM=macos_arm64
        else
            echo "Эта сборка предназначена для Mac с Apple Silicon (M1/M2/M3/M4 и новее)." >&2
            exit 1
        fi
        DIR="$HOME/Library/Application Support/AGUnlocker"
        ;;
    *) echo "Этот форк предназначен для macOS на Apple Silicon." >&2; exit 1 ;;
esac
BIN="$DIR/ag_unlocker"
TAG=""
if URL=$(curl --connect-timeout 10 --max-time 30 -fsSLo /dev/null -w '%{url_effective}' \
    "https://github.com/$REPO/releases/latest"); then
    TAG="${URL##*/}"
fi
VER="${TAG#v}"
case "$VER" in
    "" | *[!0-9._]*)
        if [ -x "$BIN" ] && [ "$(cat "$BIN.platform" 2>/dev/null || true)" = "$PLATFORM" ]; then
            echo "GitHub недоступен — запускаю сохранённую ARM64-версию." >&2
        else
            echo "Не удалось найти релиз: https://github.com/$REPO/releases" >&2
            exit 1
        fi
        ;;
    *)
        if [ ! -x "$BIN" ] || [ "$(cat "$BIN.version" 2>/dev/null || true)" != "$VER" ] ||
            [ "$(cat "$BIN.platform" 2>/dev/null || true)" != "$PLATFORM" ]; then
            echo "Скачиваю Antigravity Unlocker $VER для macOS ARM64…" >&2
            mkdir -p "$DIR"
            # Stage on the destination volume for an atomic executable replacement.
            TMP=$(mktemp -d "$DIR/.download.XXXXXX")
            trap 'rm -rf "$TMP"' 0
            trap 'exit 1' 1 2 15
            ASSET="AG_${VER}_${PLATFORM}.tar.gz"
            BASE="https://github.com/$REPO/releases/download/$TAG"
            curl --connect-timeout 10 --max-time 300 --retry 2 -fsSL "$BASE/$ASSET" -o "$TMP/$ASSET"
            curl --connect-timeout 10 --max-time 30 --retry 2 -fsSL "$BASE/$ASSET.sha256" -o "$TMP/$ASSET.sha256"
            (cd "$TMP" && shasum -a 256 -c "$ASSET.sha256") >&2
            tar xzf "$TMP/$ASSET" -C "$TMP" ag_unlocker
            if ! file "$TMP/ag_unlocker" | LC_ALL=C grep -Eq 'Mach-O.*arm64'; then
                echo "В архиве нет исполняемого файла macOS ARM64." >&2
                exit 1
            fi
            chmod 755 "$TMP/ag_unlocker"
            codesign --verify --strict "$TMP/ag_unlocker"
            mv -f "$TMP/ag_unlocker" "$BIN"
            printf '%s\n' "$VER" > "$BIN.version"
            printf '%s\n' "$PLATFORM" > "$BIN.platform"
            rm -rf "$TMP"
            trap - 0 1 2 15
        fi
        ;;
esac
# curl | sh consumes stdin; keys come from the controlling terminal.
exec "$BIN" --tui </dev/tty
