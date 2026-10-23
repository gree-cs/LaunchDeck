# 构建与打包

## 前置条件

| 依赖 | 版本 / 说明 |
| --- | --- |
| macOS | 13+（开发环境 macOS 26，Apple Silicon） |
| Rust | stable（`rustup` 安装即可） |
| Xcode Command Line Tools | 提供 `swiftc`、`codesign`、`sips` |

```bash
xcode-select --install        # 如果还没装
rustup update stable
```

## 一键构建

```bash
cd LaunchDeck
./build.sh
open dist/LaunchDeck.app
```

脚本会：

1. `cargo build --release -p launchdeck-gui`
2. `swiftc -O tools/icongen.swift -o target/release/icongen`
3. `swiftc -O tools/symbolgen.swift -o target/release/symbolgen`
4. 组装 `dist/LaunchDeck.app`：

```
dist/LaunchDeck.app/
└─ Contents/
   ├─ Info.plist
   ├─ MacOS/
   │  ├─ LaunchDeck
   │  └─ icongen
   └─ Resources/
      ├─ AppIcon.icns
      ├─ iconstyles/style0..3.png
      ├─ symbols/sym_*.png
      └─ ui/logo.png
```

   其中 `AppIcon.icns` 由 `assets/iconstyles/style0.png` 经 `sips` + `iconutil` 生成，
   不包含任何原版应用资源。运行时切换「应用图标」样式调用
   `setApplicationIconImage:`，Bundle 图标本身保持不变。

5. `codesign --force --deep --sign - dist/LaunchDeck.app`（ad-hoc 签名）

## 只跑不打包

```bash
cargo run -p launchdeck-gui --release
```

裸二进制没有 `Contents/Resources`：

- 应用图标使用 `include_bytes!` 编译进程序的 256px 兜底版本；
- 设置里的「应用图标」预览会退化为占位、SF Symbol 标签图标不可用。

## 交叉架构

默认按当前机器架构构建。需要 Intel 版本时：

```bash
rustup target add x86_64-apple-darwin
cargo build --release --target x86_64-apple-darwin -p launchdeck-gui
```

然后把产物复制进 `.app`（或者改 `build.sh` 里的 target 目录）。做成 Universal 需要 `lipo -create` 合并两份二进制。

## 安装到系统

### 方式一：install.sh（推荐）

```bash
./install.sh              # 构建 + 安装到 /Applications + 启动
./install.sh --build-only # 只构建 dist/LaunchDeck.app
./install.sh --no-open    # 安装后不自动启动
```

脚本会退出正在运行的实例，用 `ditto` 覆盖 `/Applications/LaunchDeck.app`，清除隔离标记，并重新注册到 LaunchServices。`/Applications` 不可写时会提示用 `sudo`。

### 方式二：手动拖拽

把 `dist/LaunchDeck.app` 拖进「应用程序」文件夹即可；首次启动右键 →「打开」。

### 开机自启动

在 App 内打开「开机自启动」后，会写入 `~/Library/LaunchAgents/app.launchdeck.macos.plist`，指向当前可执行文件。从 `dist/` 改安装到 `/Applications` 后，重新启动一次 App 就会刷新为新的路径。

## 发布与公证

```bash
./package.sh    # 生成 dist/LaunchDeck-<version>.zip 并打印 SHA-256
```

ad-hoc 签名在别人的机器上会触发 Gatekeeper。想做到双击即开，需要 Apple Developer ID：

```bash
codesign --force --deep --options runtime --sign "Developer ID Application: <Name> (TEAMID)" dist/LaunchDeck.app
ditto -c -k --keepParent dist/LaunchDeck.app dist/LaunchDeck-notarize.zip
xcrun notarytool submit dist/LaunchDeck-notarize.zip --keychain-profile <profile> --wait
xcrun stapler staple dist/LaunchDeck.app
```

公证后再运行一次 `./package.sh` 生成最终 zip。

## 常见问题

### 打开时提示"无法验证开发者"

ad-hoc 签名首次运行需要在「系统设置 → 隐私与安全性」里允许，或：

```bash
xattr -dr com.apple.quarantine dist/LaunchDeck.app
```

### 触控板横扫 / F4 授权

- 横扫翻页依赖 `CGEventTap`，需要「辅助功能」权限：系统设置 → 隐私与安全性 → 辅助功能 → 勾选 LaunchDeck。
- 设置页里的「授予权限」按钮会直接跳转到对应面板。
- F4 若被系统分配给「启动台」，需要在 键盘 → 快捷键 里解绑。

### 快捷键没反应

- 确认 App 正在运行（菜单栏图标存在）；
- 两个实例会争抢同一个热键：程序内置单实例保护，重复启动会直接退出；
- 若原版 LaunchOS 也在运行，它的全局热键会先占用 Option+Space。

### 从旧名字升级

首次启动会自动把 `~/Library/Application Support/LaunchOS/{settings,layout}.json` 复制到新的 `LaunchDeck` 目录（不覆盖已有文件），图标缓存也会复制到 `~/Library/Caches/LaunchDeck/`，并删除旧的 `app.launchos.rust.plist`。旧目录本身不会删除。

### Dock 里出现多个 LaunchDeck

macOS 会为不同路径的启动各留一条 Dock 条目（`/Applications`、`dist/`、裸二进制）。右键条目 → 选项 → 从程序坞中移除即可；以后统一用 `open dist/LaunchDeck.app` 启动。

### 图标不更新

Dock 会缓存 App 的 Bundle 图标。重新打包后如果程序坞里还是旧图标：

```bash
killall Dock
```

或者强制重新注册：

```bash
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f dist/LaunchDeck.app
```

应用运行时的图标由 `setApplicationIconImage:` 设置（启动后在第 2/12/40 帧补设，
避开 NSApp 尚未完成启动的窗口期），切换「设置 → 通用 → 应用图标」会立即生效。

删除应用图标缓存后重启：

```bash
rm -rf ~/Library/Caches/LaunchDeck/icons-v2
```

### 布局乱了想恢复

```bash
rm ~/Library/Application\ Support/LaunchDeck/layout.json
```

下次启动会按扫描结果重建；若想恢复原版布局，用设置 → 高级 → 系统 Launchpad 数据 → 恢复。

## 发布建议

推 GitHub 前可以检查：

- `cargo fmt` / `cargo clippy`（当前未强制 CI）
- 不要把 `dist/`、`target/` 提交进仓库（已在 `.gitignore`）
- 不需要包含任何原版二进制或资源
- 发布页附上 `package.sh` 生成的 SHA-256；仓库根目录的 `LICENSE` 为 MIT
