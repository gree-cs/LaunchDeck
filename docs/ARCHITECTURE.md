# 架构说明

本文档描述项目的模块划分、每帧的渲染/交互流程，以及所有原生（AppKit / Carbon / CoreGraphics）集成点，方便二次开发。

## 1. 工作区划分

```
launchdeck-core   纯逻辑，不依赖 UI
launchdeck-gui    eframe + egui 前端（进程入口）
launchdeck-ffi    C ABI 导出，供 Swift / 其他语言调用
```

`launchdeck-core` 只依赖 `serde` / `dirs` / `walkdir`，可以在其他前端（比如 SwiftUI）里复用扫描、配置和布局模型。

## 2. 数据结构

```
LaunchLayout
  pages: Vec<Page>
    items: Vec<Item>
      Item::App    { app: AppRef { bundle_id, name, path } }
      Item::Folder { folder: Folder { id, name, apps: Vec<AppRef> } }
  sort_mode / hidden / last_used / hide_names / columns / rows / browsing_style

AppSettings
  display_mode / hotkey / show_dock_icon / show_menu_bar_icon / launch_at_login
  theme / grid_size / background / icon_style / hot_corner / hide_after_launch
  show_on / return_home / option_uninstall / hide_hidden_in_search
```

- 布局持久化在 `layout.json`，设置持久化在 `settings.json`。
- `layout_store` 负责：加载/保存、首次从扫描结果构建、应用增删后合并、排序、重排。

## 3. 运行时结构

```
main()
 └─ eframe::run_native
     └─ LaunchDeckApp::update()   每帧执行
         ├─ 处理原生事件（热键 / 菜单 / 触发角 / F4 / 横扫）
         ├─ 扫描线程轮询（应用列表 + 图标导出结果）
         ├─ 壁纸纹理轮询
         ├─ 键盘处理（Esc / 方向键 / 搜索）
         ├─ ui::draw()            绘制启动台 / 设置 / 卸载 / 文件夹
         └─ 动画推进（打开淡入、关闭淡出、自动收起判断）
```

### 启动台渲染

`ui::draw()` 每帧：

1. `paint_background`：绘制模糊壁纸（窗口模式下用圆角版本），叠压暗；
2. `grid_page`：计算 `Geom`（列距、行距、图标尺寸），遍历当前页绘制图标；
3. `search_bar` / `page_dots`；
4. 文件夹浮层、设置窗口、卸载窗口（独立 `egui::Area`，层级 `Tooltip`/`Foreground`）；
5. 整个 CentralPanel 按 `show_anim` 设置不透明度，实现打开/关闭的淡入淡出。

### 窗口尺寸与模式

- 全屏：`Decorations(false)` + `InnerSize(屏幕)` + `OuterPosition(0,0)` + 窗口层级 25（覆盖菜单栏）。
- 窗口：`InnerSize(1200×800)` + 居中 + 层级 21。
- 设置：进入时把同一个窗口缩到 400×标签页高度并居中；退出时恢复当前显示模式。
- 窗口透明（`with_transparent`），设置页只画面板，四周透出真实桌面。

## 4. 交互实现

### 拖拽

- 图标用 `Sense::click_and_drag()`，`drag_started` 记录 `DragState { item, from_page, from_index }`；
- 绘制时如果 `app.drag` 指向本页，跳过源图标、后续图标整体前移一位（视觉上"原位空出"），指针位置绘制跟随的幽灵图标，悬停格高亮；
- `drag_stopped` 时把"预览格"换算回原始下标，调用 `drop_at()`：
  - 目标是文件夹 → 加入；
  - 目标是另一个 App → 合并成新文件夹并进入重命名；
  - 其他 → 普通移动。

### 文件夹

- `folder_view()` 绘制：背景遮罩（同一张壁纸整幅覆盖 + 同档压暗）、面板、标题、3×3 内网格；
- 面板高度按行数收缩（`12 + 行数 × 178.7`，最多 5 行），垂直居中；
- 拖拽时指针离开面板即开始收起动画（拖回取消），松手后应用落到指针所在格；
- 双击标题或右键重命名 → 标题就地变成输入框。

### 多选

- `selected: HashSet<bundle_id>`；
- ⌘+单击切换、⌘+拖拽框选（`marquee`）、右键全选/取消；
- 选中态绘制为图标外一块半透明圆角玻璃（无描边）；
- 批量操作：隐藏、移入废纸篓、用所选应用新建文件夹、整体拖动。

## 5. 原生集成

全部集中在 `launchdeck-gui/src/native.rs`，通过 `objc_msgSend` / Carbon / CoreGraphics 直接调用，不引入额外依赖。

| 功能 | API |
| --- | --- |
| 全局快捷键 | `RegisterEventHotKey` + `InstallEventHandler`（Carbon） |
| F4 | 同上，第二个 EventHotKeyID |
| 触控板横扫 | `CGEventTapCreate` 监听 `scrollWheel` 的横向分量 |
| 菜单栏图标 | `NSStatusBar.statusItemWithLength`，按钮使用 action 区分左右键 |
| 菜单栏点击行为 | 左键 → 打开启动台；右键 → `popUpContextMenu:` |
| Dock reopen | 给 `NSApplication.delegate` 动态挂 `applicationShouldHandleReopen:hasVisibleWindows:` |
| 应用激活 | `NSRunningApplication.activateWithOptions:` |
| 窗口层级 | `NSWindow.setLevel:`（全屏 25 / 窗口 21 / 默认 0） |
| 菜单栏 & Dock 隐藏 | `setPresentationOptions:`（`HideMenuBar|HideDock`，必须成对） |
| Dock 图标显示/隐藏 | `NSApplication.setActivationPolicy:` |
| 应用自身图标 | `setApplicationIconImage:` + 状态栏按钮 `setImage:` |
| 开机自启动 | 写入 `~/Library/LaunchAgents/*.plist` |
| 壁纸路径 | `NSWorkspace.desktopImageURL(for:)` |
| SF Symbol / logo | 构建期渲染为 PNG，运行时按纹理加载 |

## 6. 图标资源

| 资源 | 生成方式 | 位置 |
| --- | --- | --- |
| 应用图标 | `icongen`（NSWorkspace，512px）批量导出 | `~/Library/Caches/LaunchDeck/icons-v2/` |
| 设置标签图标 | `symbolgen` 渲染 SF Symbols（2×） | bundle `Resources/symbols/` |
| 应用图标样式 | 用户提供的设计图 → 裁切 + 圆角透明蒙版 | bundle `Resources/iconstyles/`，256px 版本 `include_bytes!` |
| 关于页 logo | 设计图裁切 | bundle `Resources/ui/logo.png` |

`icons.rs` 负责纹理缓存：`IconCache` 按路径缓存应用图标，`symbol()` / `asset()` 分别缓存 SF Symbols 与资源 PNG。

## 7. 动画状态

| 字段 | 含义 |
| --- | --- |
| `show_anim` | 启动台整体不透明度 0→1（0.36s 缓出），关闭时 1→0（0.24s） |
| `folder_anim` | 文件夹面板缩放/不透明度 0→1（0.26s）与回调（0.18s） |
| `page_anim` | 翻页滑动进度（0.24s） |
| `closing` | 正在淡出，结束后才真正 `Visible(false)` |

`ignore_focus_frames` / `unfocused_frames` / `was_active` 用于避免"刚唤出就被判失败焦收起"。

## 8. 布局导入

`orig_layout.rs` 读取原版 `~/Library/Application Support/LaunchOS/LaunchOS.sqlite`（Core Data：`ZAPPENTITY` / `ZGROUPENTITY` / `ZSOURCEENTITY`），还原分页、文件夹与顺序。导入只发生一次，之后布局完全由 `layout.json` 维护。
