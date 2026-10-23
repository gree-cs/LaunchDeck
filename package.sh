#!/bin/bash
# 构建并生成可发布到 GitHub Releases 的 zip
set -euo pipefail
cd "$(dirname "$0")"

APP_NAME="LaunchDeck"

./build.sh

VERSION=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "dist/$APP_NAME.app/Contents/Info.plist")
OUT="dist/$APP_NAME-$VERSION.zip"

rm -f "$OUT"
# --keepParent 让解压后得到 LaunchDeck.app，而不是散落的 Contents
ditto -c -k --keepParent "dist/$APP_NAME.app" "$OUT"

echo "✅ 已生成: $OUT"
shasum -a 256 "$OUT"
echo
echo "提示：未使用 Developer ID 公证的包，用户首次打开需要右键 →「打开」，"
echo "      或在「系统设置 → 隐私与安全性」里选择「仍要打开」。"
