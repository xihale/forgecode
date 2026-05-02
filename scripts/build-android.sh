#!/usr/bin/env bash
# build-android.sh — Cross-compile Rust to Android.
#
#   ./build-android.sh                          # aarch64, API 28, release
#   ./build-android.sh -t armv7-linux-androideabi -a 21 --debug
#   ./build-android.sh -t all                   # all 4 Android targets
set -euo pipefail

TARGET="aarch64-linux-android"
API=28
PROFILE="release"
PACKAGE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--target)  TARGET="$2"; shift 2 ;;
        -a|--api)     API="$2"; shift 2 ;;
        -p|--package) PACKAGE="$2"; shift 2 ;;
        --ndk)        NDK_PATH="$2"; shift 2 ;;
        --debug)      PROFILE="dev"; shift ;;
        -h|--help)    sed -n '3,6p' "$0" | sed 's/^# //' ; exit ;;
        *)            echo "Unknown: $1"; exit 1 ;;
    esac
done

ALL_TARGETS=(aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android)

if [[ "$TARGET" == "all" ]]; then
    for t in "${ALL_TARGETS[@]}"; do
        echo ">>> $t"
        "$0" --target "$t" --api "$API" --ndk "${NDK_PATH:-}" \
             $([[ "$PROFILE" == "dev" ]] && echo --debug) \
             $([[ -n "$PACKAGE" ]] && echo --package "$PACKAGE") || true
    done
    exit 0
fi

# ── Find NDK ────────────────────────────────────────────────────────────────
if [[ -z "${NDK_PATH:-}" || ! -d "${NDK_PATH:-}" ]]; then
    for dir in \
        "${ANDROID_NDK_HOME:-}" \
        "${ANDROID_HOME:+$ANDROID_HOME/ndk}" \
        "${ANDROID_SDK_ROOT:+$ANDROID_SDK_ROOT/ndk}" \
        "$HOME/Android/Sdk/ndk" \
        "$HOME/Android/android-sdk/ndk"; do
        [[ -z "$dir" || ! -d "$dir" ]] && continue
        NDK_PATH=$(ls -1d "$dir"/*/ 2>/dev/null | sort -V | tail -1)
        [[ -n "$NDK_PATH" ]] && NDK_PATH="${NDK_PATH%/}" && break
    done
fi

if [[ -z "${NDK_PATH:-}" || ! -d "${NDK_PATH:-}" ]]; then
    echo "Android NDK not found."
    echo "  Install: Android Studio → SDK Manager → NDK (Side by side)"
    echo "  Or:      export ANDROID_NDK_HOME=/path/to/ndk"
    echo "  Or:      $0 --ndk /path/to/ndk"
    echo "  Download: https://developer.android.com/ndk/downloads"
    exit 1
fi
echo "✓ NDK: $NDK_PATH"

# ── Toolchain paths ─────────────────────────────────────────────────────────
HOST="$(uname -s | tr A-Z a-z)-$(uname -m)"
TC="$NDK_PATH/toolchains/llvm/prebuilt/$HOST"

declare -A PREFIX=(
    [aarch64-linux-android]=aarch64-linux-android
    [armv7-linux-androideabi]=armv7a-linux-androideabi
    [x86_64-linux-android]=x86_64-linux-android
    [i686-linux-android]=i686-linux-android
)
P="${PREFIX[$TARGET]:?Unsupported target: $TARGET (supported: ${ALL_TARGETS[*]})}"

CC="$TC/bin/${P}${API}-clang"
AR="$TC/bin/llvm-ar"
[[ -x "$CC" ]] || { echo "✗ $CC not found (API $API unsupported by this NDK?)"; exit 1; }

# ── Install target & build ──────────────────────────────────────────────────
rustup target list --installed | grep -q "^$TARGET$" || rustup target add "$TARGET"

KEY="${TARGET//-/_}"
DIR=$([[ "$PROFILE" == "release" ]] && echo release || echo debug)

echo "✓ $TARGET API$API ($PROFILE)"
env "CC_${KEY}=$CC" "AR_${KEY}=$AR" "CARGO_TARGET_${KEY^^}_LINKER=$CC" \
    cargo build --target "$TARGET" $([[ "$PROFILE" == "release" ]] && echo --release) \
                $([[ -n "$PACKAGE" ]] && echo --package "$PACKAGE")

# ── Report ───────────────────────────────────────────────────────────────────
BIN=$(find "target/$TARGET/$DIR" -maxdepth 1 -type f -executable ! -name '*.d' ! -name '*.so' 2>/dev/null | head -1)
if [[ -n "$BIN" ]]; then
    echo "✓ $(du -h "$BIN" | cut -f1)  $BIN"
fi
