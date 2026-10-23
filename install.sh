#!/bin/bash
# 构建并安装到 /Applications
set -euo pipefail
cd "$(dirname "$0")"

APP_NAME="LaunchDeck"
DEST="/Applications/$APP_NAME.app"
BUILD_ONLY=0
NO_OPEN=0

for arg in "$@"; do
    case "$arg" in
        --build-only) BUILD_ONLY=1 ;;
        --no-open) NO_OPEN=1 ;;
        -h|--help)
            echo "用法: ./install.sh [--build-only] [--no-open]"
            echo "  --build-only  只构建 dist/$APP_NAME.app，不安装"
            echo "  --no-open     安装后不自动启动"
            exit 0
            ;;
        *)
            echo "未知参数: $arg" >&2
            exit 2
            ;;
    esac
done

./build.sh

if [ "$BUILD_ONLY" = "1" ]; then
    echo "已构建 dist/$APP_NAME.app"
    exit 0
fi

# 先退出正在运行的实例（无论它来自 dist 还是 /Applications）
osascript -e "tell application \"$APP_NAME\" to quit" >/dev/null 2>&1 || true
pkill -f "$APP_NAME.app/Contents/MacOS/$APP_NAME" >/dev/null 2>&1 || true
sleep 1

if [ -w /Applications ]; then
    rm -rf "$DEST"
    ditto "dist/$APP_NAME.app" "$DEST"
else
    echo "写入 /Applications 需要管理员权限"
    sudo rm -rf "$DEST"
    sudo ditto "dist/$APP_NAME.app" "$DEST"
fi

# 本地构建的包没有公证，去掉隔离标记可避免首次打开被拦
xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true

LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
[ -x "$LSREGISTER" ] && "$LSREGISTER" -f "$DEST" >/dev/null 2>&1 || true

if [ "$NO_OPEN" = "0" ]; then
    open "$DEST"
fi

echo "✅ 已安装: $DEST"
echo "   开机自启动/数据目录等仍由 App 自己维护；再次运行本脚本可直接覆盖升级。"
