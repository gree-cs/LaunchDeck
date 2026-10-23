#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"

echo "🔨 构建 LaunchDeck (Rust)..."
cargo build --release -p launchdeck-gui

echo "🎨 构建图标导出辅助程序..."
mkdir -p target/release
swiftc -O tools/icongen.swift -o target/release/icongen

echo "🔣 渲染 SF Symbols..."
swiftc -O tools/symbolgen.swift -o target/release/symbolgen

echo "📦 打包 .app..."
APP=dist/LaunchDeck.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/LaunchDeck "$APP/Contents/MacOS/LaunchDeck"
cp target/release/icongen "$APP/Contents/MacOS/icongen"
# 设置页等界面图标：直接渲染系统 SF Symbols
mkdir -p "$APP/Contents/Resources/symbols"
target/release/symbolgen "$APP/Contents/Resources/symbols" >/dev/null
# 应用图标样式（用户提供的设计图，非原版资源）
mkdir -p "$APP/Contents/Resources/iconstyles" "$APP/Contents/Resources/ui"
cp assets/iconstyles/*.png "$APP/Contents/Resources/iconstyles/"
cp assets/ui/*.png "$APP/Contents/Resources/ui/"

# 应用图标：用我们自己的图标样式（style0）生成 AppIcon.icns，
# 这样 Finder / Dock 在 App 未运行时也能看到正确的图标。
ICONSET="target/AppIcon.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
for spec in \
    "16 icon_16x16" "32 icon_16x16@2x" \
    "32 icon_32x32" "64 icon_32x32@2x" \
    "128 icon_128x128" "256 icon_128x128@2x" \
    "256 icon_256x256" "512 icon_256x256@2x" \
    "512 icon_512x512" "1024 icon_512x512@2x"; do
    size=${spec%% *}
    name=${spec#* }
    sips -z "$size" "$size" assets/iconstyles/style0.png --out "$ICONSET/$name.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>LaunchDeck</string>
    <key>CFBundleDisplayName</key><string>LaunchDeck</string>
    <key>CFBundleExecutable</key><string>LaunchDeck</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIdentifier</key><string>app.launchdeck.macos</string>
    <key>CFBundleVersion</key><string>0.3.0</string>
    <key>CFBundleShortVersionString</key><string>0.3.0</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
</dict>
</plist>
PLIST

echo "🔏 签名..."
codesign --force --deep --sign - "$APP"
codesign --verify --strict "$APP"

echo "✅ 完成: $APP"
echo "   运行: open $APP"
