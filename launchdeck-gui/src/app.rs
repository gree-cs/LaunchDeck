//! LaunchDeck 主应用状态

use crate::icons::IconCache;
use crate::native;
use crate::layout::*;
use crate::layout_store;
use crate::ui;
use eframe::egui;
use launchdeck_core::{AppInfo, AppSettings, DisplayMode, PinchGesture, ReturnHome, ScreenChoice, Theme};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Tab { Launchpad, Uninstall, Settings }

/// 唤出启动台时自动重扫的最小间隔（扫描含全量目录大小统计，避免每次唤出都跑）
const AUTO_RESCAN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// 深度卸载确认面板的状态
pub struct UninstallDetail {
    pub bid: String,
    pub name: String,
    /// 待删除的应用路径（替身应用为替身文件本身）
    pub path: String,
    pub residues: Vec<launchdeck_core::uninstall::Residue>,
    pub checked: Vec<bool>,
    /// 应用正在运行，需要先退出
    pub running: bool,
    /// 已尝试正常退出的次数（第二次起强制退出）
    pub attempts: u8,
}

#[derive(Clone)]
pub struct UpdateInfo {
    pub tag: String,
    pub url: String,
    pub notes: String,
}

/// 拖拽状态
pub struct DragState {
    pub item: Item,
    pub from_page: usize,
    pub from_index: usize,
    /// 从文件夹视图拖出时为 Some(文件夹 id)，落点由 drag_app_out_of_folder 处理
    pub from_folder: Option<String>,
}

pub struct LaunchDeckApp {
    // 扫描结果
    pub apps: Vec<AppInfo>,
    pub scanning: bool,
    pub scan_rx: Option<Receiver<(Vec<AppInfo>, HashMap<String, PathBuf>)>>,

    // 布局
    pub layout: LaunchLayout,
    pub page: usize,

    // 交互
    pub search: String,
    pub selected: HashSet<String>,
    pub drag: Option<DragState>,
    pub editing_folder: Option<String>,     // 正在重命名的文件夹 id
    pub edit_text: String,
    pub focus_editing: bool,                // 下一帧聚焦重命名输入框
    pub open_folder: Option<String>,        // 打开的文件夹（全屏浮层）
    pub confirm_delete: Option<String>,
    pub scroll_accum: f32,                  // 触控板横扫累计
    pub search_focus: bool,                 // 请求聚焦搜索框
    pub folder_closing: bool,               // 文件夹正在收起
    pub page_prev: usize,                   // 翻页动画的上一页
    pub page_anim: f32,                     // 翻页动画进度 0..1
    pub page_dir: f32,                      // 翻页方向 +1 / -1
    pub folder_scroll: f32,                 // 文件夹内滚动（行）
    pub glow_pos: egui::Pos2,               // 平滑跟随的光斑位置
    pub glow_alpha: f32,                    // 光斑透明度
    pub marquee: Option<(egui::Pos2, egui::Pos2)>, // 框选
    pub confirm_batch: bool,
    pub multi_drag: bool,

    // 设置
    pub settings: AppSettings,

    pub tab: Tab,
    pub settings_section: usize,
    pub status: String,
    pub icons: IconCache,
    /// 窗口是否可见（全局快捷键/触发角切换）
    pub visible: bool,
    pub ctx: Option<egui::Context>,
    /// 首帧应用显示模式
    pub first_frame: bool,
    pub prev_tab: Tab,
    /// 刚显示时忽略几帧失焦事件，避免刚唤出就被判定为失焦而收起
    pub ignore_focus_frames: u8,
    pub unfocused_frames: u8,
    /// 上一次检查时 App 是否处于前台（只有"前台→后台"才自动收起）
    pub was_active: bool,
    /// 当前窗口尺寸（拖放落点计算用）
    pub screen_size: Option<egui::Vec2>,
    /// F4 / 触控板监听是否已安装（首帧后）
    pub native_ready: bool,
    /// 启动后补设应用图标的帧计数（NSApp 完成启动前设置 Dock 图标不生效）
    pub icon_apply_frames: u8,
    pub prev_section: usize,
    /// 全局快捷键是否注册成功
    pub hotkey_ok: bool,
    /// 全屏时剩余的位置校正帧数
    pub reposition: u8,
    /// 当前已应用的主题
    pub applied_theme: Option<Theme>,
    /// 模糊壁纸背景
    pub wallpaper: Option<egui::TextureHandle>,
    /// 窗口模式用的圆角版本
    pub wallpaper_win: Option<egui::TextureHandle>,
    /// 文件夹背板用的重模糊版本
    pub wallpaper_heavy: Option<egui::TextureHandle>,
    /// 文件夹开合动画进度 0..1
    pub folder_anim: f32,
    /// 打开时的淡入进度（0..1）
    pub show_anim: f32,
    /// 正在播放淡出动画
    pub closing: bool,
    /// 文件夹内正在拖拽（拖出时先播收起动画但不销毁）
    pub folder_drag_active: bool,
    /// 顶部柔光纹理
    pub glow: Option<egui::TextureHandle>,
    pub wallpaper_rx: Option<Receiver<Option<egui::ColorImage>>>,
    /// 当前壁纸的 path:mtime，用于检测系统壁纸变化后重新模糊
    pub wallpaper_key: String,
    /// 上次扫描完成时间（唤出启动台时限流自动重扫）
    pub last_scan_done: Option<std::time::Instant>,
    /// 上次隐藏时间（「回到主页面」判断用）
    pub last_hidden_at: Option<std::time::Instant>,
    /// 类型筛选（LSApplicationCategoryType 原始值）
    pub category_filter: Option<String>,
    /// 搜索栏「⋯」主菜单是否打开
    pub main_menu_open: bool,
    /// 主菜单「类型」手风琴是否展开
    pub main_menu_types_open: bool,
    /// 主菜单打开时的帧号（避免打开当帧的点击被当作「点击外部」）
    pub menu_open_at_frame: u64,
    /// 「整理布局」页面选择弹窗
    pub tidy_popup_open: bool,
    pub tidy_selected: HashSet<usize>,
    /// 「整理空位」的作用域：0=当前页 1=全部页
    pub tidy_scope: usize,
    /// 深度卸载确认面板
    pub uninstall_detail: Option<UninstallDetail>,
    /// 更新检查
    pub update_rx: Option<Receiver<Option<UpdateInfo>>>,
    pub update_info: Option<UpdateInfo>,
    pub update_status: String,
    /// 滚动浏览模式的纵向偏移
    pub browse_scroll: f32,
    /// 正在重命名的应用（bundle_id 或路径）
    pub renaming_app: Option<String>,
    /// 拖拽时各格子的当前显示位置（指数平滑，图标滑动到预览格而不是跳变）
    pub drag_offsets: HashMap<(usize, usize), (f32, f32)>,
    /// 打开的文件夹视图面板区域（从文件夹拖出的落点判定用）
    pub folder_panel_rect: Option<egui::Rect>,
}

impl LaunchDeckApp {
    /// 测试用：跳过原生集成与窗口创建的最小实例
    pub fn new_headless() -> Self {
        Self {
            apps: Vec::new(),
            scanning: false,
            scan_rx: None,
            layout: LaunchLayout::default(),
            page: 0,
            search: String::new(),
            selected: HashSet::new(),
            drag: None,
            editing_folder: None,
            edit_text: String::new(),
            focus_editing: false,
            open_folder: None,
            confirm_delete: None,
            scroll_accum: 0.0,
            search_focus: false,
            folder_closing: false,
            page_prev: 0,
            page_anim: 1.0,
            page_dir: 1.0,
            folder_scroll: 0.0,
            glow_pos: egui::pos2(0.0, 0.0),
            glow_alpha: 0.0,
            marquee: None,
            confirm_batch: false,
            multi_drag: false,
            settings: launchdeck_core::get_settings(),
            tab: Tab::Launchpad,
            settings_section: 0,
            status: String::new(),
            icons: IconCache::default(),
            visible: false,
            ctx: None,
            first_frame: false,
            prev_tab: Tab::Launchpad,
            ignore_focus_frames: 0,
            unfocused_frames: 0,
            was_active: false,
            screen_size: None,
            native_ready: false,
            icon_apply_frames: 0,
            prev_section: 0,
            hotkey_ok: false,
            reposition: 0,
            applied_theme: None,
            wallpaper: None,
            wallpaper_win: None,
            wallpaper_heavy: None,
            folder_anim: 0.0,
            show_anim: 0.0,
            closing: false,
            folder_drag_active: false,
            glow: None,
            wallpaper_rx: None,
            wallpaper_key: String::new(),
            last_scan_done: None,
            last_hidden_at: None,
            category_filter: None,
            main_menu_open: false,
            main_menu_types_open: false,
            menu_open_at_frame: 0,
            tidy_popup_open: false,
            tidy_selected: HashSet::new(),
            tidy_scope: 0,
            uninstall_detail: None,
            update_rx: None,
            update_info: None,
            update_status: String::new(),
            browse_scroll: 0.0,
            renaming_app: None,
            drag_offsets: HashMap::new(),
            folder_panel_rect: None,
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut layout = layout_store::load().unwrap_or_default();
        layout_store::normalize(&mut layout);

        let mut app = Self {
            apps: Vec::new(),
            scanning: false,
            scan_rx: None,
            layout,
            page: 0,
            search: String::new(),
            selected: HashSet::new(),
            drag: None,
            editing_folder: None,
            edit_text: String::new(),
            focus_editing: false,
            open_folder: None,
            confirm_delete: None,
            scroll_accum: 0.0,
            search_focus: false,
            folder_closing: false,
            page_prev: 0,
            page_anim: 1.0,
            page_dir: 1.0,
            folder_scroll: 0.0,
            glow_pos: egui::pos2(0.0, 0.0),
            glow_alpha: 0.0,
            marquee: None,
            confirm_batch: false,
            multi_drag: false,
            settings: launchdeck_core::get_settings(),
            tab: Tab::Launchpad,
            settings_section: 0,
            status: "就绪".into(),
            icons: IconCache::default(),
            visible: true,
            ctx: None,
            first_frame: true,
            prev_tab: Tab::Launchpad,
            ignore_focus_frames: 0,
            unfocused_frames: 0,
            was_active: false,
            screen_size: None,
            native_ready: false,
            icon_apply_frames: 0,
            prev_section: 0,
            hotkey_ok: false,
            reposition: 0,
            applied_theme: None,
            wallpaper: None,
            wallpaper_win: None,
            wallpaper_heavy: None,
            folder_anim: 0.0,
            show_anim: 0.0,
            closing: false,
            folder_drag_active: false,
            glow: Some(make_glow(&cc.egui_ctx)),
            wallpaper_rx: None,
            wallpaper_key: String::new(),
            last_scan_done: None,
            last_hidden_at: None,
            category_filter: None,
            main_menu_open: false,
            main_menu_types_open: false,
            menu_open_at_frame: 0,
            tidy_popup_open: false,
            tidy_selected: HashSet::new(),
            tidy_scope: 0,
            uninstall_detail: None,
            update_rx: None,
            update_info: None,
            update_status: String::new(),
            browse_scroll: 0.0,
            renaming_app: None,
            drag_offsets: HashMap::new(),
            folder_panel_rect: None,
        };

        // 原生集成
        native::set_context(cc.egui_ctx.clone());
        native::force_dark_appearance();
        native::set_hot_corner(app.settings.hot_corner);
        native::start_corner_watcher();
        let (key, mods) = app
            .settings
            .hotkey
            .as_ref()
            .map(|h| (h.key_code, h.modifiers))
            .unwrap_or((49, 0x2000));
        app.hotkey_ok = native::register_hotkey(key, mods);
        native::remove_legacy_launch_agent();
        if app.settings.launch_at_login {
            let _ = native::set_launch_at_login(true);
        }
        if !app.settings.show_dock_icon {
            native::set_dock_icon_visible(false);
        }
        if app.settings.show_menu_bar_icon {
            native::install_menu_bar();
        }
        // 壁纸背景（后台加载）
        let icon_idx = Self::icon_index(app.settings.icon_style);
        native::set_app_icon(icon_idx);
        app.reload_wallpaper_if_changed();
        if let Ok(t) = std::env::var("LAUNCHDECK_TAB") {
            if t == "settings" {
                app.tab = Tab::Settings;
            }
        }
        app.start_scan();
        if app.settings.auto_update {
            app.spawn_update_check();
        }
        app
    }

    // ---------- 扫描 ----------
    pub fn start_scan(&mut self) {
        if self.scanning { return; }
        self.scanning = true;
        self.status = "正在扫描应用…".into();
        let (tx, rx) = channel();
        self.scan_rx = Some(rx);
        thread::spawn(move || {
            let mut list = launchdeck_core::AppScanner::default().scan().unwrap_or_default();
            list = normalize_scan(list);
            let stamp = |p: &std::path::Path| -> u64 {
                std::fs::metadata(p)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            };
            let mut paths: Vec<(String, u64)> = list
                .iter()
                .map(|a| (a.path.to_string_lossy().to_string(), stamp(&a.path)))
                .collect();
            // 布局里来自原版数据、但扫描不到的应用（例如无 .app 后缀的卸载器）
            // 也要导出图标，才能和原版逐格一致。
            if let Some(saved) = crate::layout_store::load() {
                let mut extra: Vec<String> = Vec::new();
                let mut collect = |r: &crate::layout::AppRef| {
                    if !paths.iter().any(|(p, _)| p == &r.path)
                        && !extra.iter().any(|p| p == &r.path)
                    {
                        extra.push(r.path.clone());
                    }
                };
                for page in &saved.pages {
                    for it in &page.items {
                        match it {
                            crate::layout::Item::App { app } => collect(app),
                            crate::layout::Item::Folder { folder } => {
                                for a in &folder.apps {
                                    collect(a);
                                }
                            }
                        }
                    }
                }
                for p in extra {
                    if std::path::Path::new(&p).exists() {
                        paths.push((p.clone(), stamp(std::path::Path::new(&p))));
                    }
                }
            }
            let manifest = crate::icons::generate(&paths);
            let _ = tx.send((list, manifest));
        });
    }

    pub fn poll_scan(&mut self) {
        if let Some(rx) = &self.scan_rx {
            if let Ok((list, manifest)) = rx.try_recv() {
                self.scan_rx = None;
                self.scanning = false;
                self.last_scan_done = Some(std::time::Instant::now());
                self.apps = list;
                self.icons.set_manifest(manifest);
                self.resolve_paths();
                layout_store::build_from_apps(&self.apps, &mut self.layout);
                layout_store::apply_sort(&mut self.layout, &self.settings);
                self.status = format!("已加载 {} 个应用", self.apps.len());
                self.clamp_page();
                let _ = layout_store::save(&self.layout);
            }
        }
    }

    /// 原版界面始终为深色外观
    pub fn is_dark(&self) -> bool {
        true
    }

    /// 壁纸不是写死的：每次显示启动台时检查系统壁纸是否变化，变了就重新模糊
    pub fn reload_wallpaper_if_changed(&mut self) {
        let Some(path) = crate::wallpaper::desktop_wallpaper_path() else {
            return;
        };
        let mt = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let key = format!("{path}:{mt}");
        if key == self.wallpaper_key {
            return;
        }
        self.wallpaper_key = key;
        let (tx, rx) = channel();
        self.wallpaper_rx = Some(rx);
        thread::spawn(move || {
            let _ = tx.send(crate::wallpaper::load_blurred(&path));
        });
    }

    fn poll_wallpaper(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.wallpaper_rx {
            if let Ok(img) = rx.try_recv() {
                self.wallpaper_rx = None;
                if let Some(img) = img {
                    // 窗口模式：1200×800、圆角 20 的版本
                    let win_img = crate::wallpaper::rounded_copy(&img, 1200, 800, 20.0);
                    self.wallpaper_win = Some(ctx.load_texture(
                        "wallpaper-win",
                        win_img,
                        egui::TextureOptions::LINEAR,
                    ));
                    let heavy = crate::wallpaper::heavy_blur(&img);
                    self.wallpaper_heavy = Some(ctx.load_texture(
                        "wallpaper-heavy",
                        heavy,
                        egui::TextureOptions::LINEAR,
                    ));
                    self.wallpaper = Some(ctx.load_texture(
                        "wallpaper",
                        img,
                        egui::TextureOptions::LINEAR,
                    ));
                    ctx.request_repaint();
                }
            }
        }
    }

    /// 用扫描结果修正布局中失效的路径（例如 Safari 的 cryptex 路径）
    fn resolve_paths(&mut self) {
        use std::collections::HashMap;
        use std::path::Path;

        let mut map: HashMap<String, (String, String)> = HashMap::new();
        for a in &self.apps {
            let bid = if a.bundle_id.is_empty() {
                a.path.to_string_lossy().to_string()
            } else {
                a.bundle_id.clone()
            };
            map.insert(
                bid,
                (a.path.to_string_lossy().to_string(), a.name.clone()),
            );
        }
        for page in &mut self.layout.pages {
            for item in &mut page.items {
                match item {
                    Item::App { app } => fix(app, &map),
                    Item::Folder { folder } => {
                        for app in &mut folder.apps {
                            fix(app, &map);
                        }
                    }
                }
            }
        }

        fn fix(a: &mut AppRef, map: &HashMap<String, (String, String)>) {
            if !Path::new(&a.path).exists() {
                if let Some((p, n)) = map.get(&a.bundle_id) {
                    a.path = p.clone();
                    if a.name.is_empty() {
                        a.name = n.clone();
                    }
                }
            }
        }
    }

    pub fn clamp_page(&mut self) {
        if self.page >= self.layout.page_count() {
            self.page = self.layout.page_count().saturating_sub(1);
        }
    }

    // ---------- 查询 ----------
    pub fn search_results(&self) -> Vec<Item> {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() { return vec![]; }
        let mut out = Vec::new();
        for page in &self.layout.pages {
            for it in &page.items {
                match it {
                    Item::App { app } => {
                        if app.name.to_lowercase().contains(&q)
                            || app.bundle_id.to_lowercase().contains(&q)
                        {
                            out.push(it.clone());
                        }
                    }
                    Item::Folder { folder } => {
                        if folder.name.to_lowercase().contains(&q) {
                            out.push(it.clone());
                        } else {
                            for a in &folder.apps {
                                if a.name.to_lowercase().contains(&q) {
                                    out.push(Item::app(a.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
        out
    }

    // ---------- 启动 ----------
    pub fn open(&mut self, app: &AppRef) {
        let _ = Command::new("/usr/bin/open").arg(&app.path).spawn();
        self.layout
            .last_used
            .insert(app.bundle_id.clone(), layout_store::now_ts());
        let _ = layout_store::save(&self.layout);
        if self.settings.hide_after_launch {
            self.hide_window();
        }
    }

    pub fn hide_window(&mut self) {
        if !self.visible || self.closing {
            return;
        }
        // 先播放 0.15s 淡出，结束后再真正隐藏窗口（平滑，不闪）
        self.closing = true;
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }

    fn finish_hide(&mut self, ctx: &egui::Context) {
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!("[app] finish_hide start");
        }
        self.closing = false;
        self.visible = false;
        self.show_anim = 0.0;
        self.last_hidden_at = Some(std::time::Instant::now());
        self.main_menu_open = false;
        crate::native::set_menu_bar_hidden(false);
        crate::native::set_window_level(0);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!("[app] finish_hide done");
        }
    }

    /// 打开启动台（与设置解耦：即使之前停在设置页，也切回启动台）
    pub fn show_launchpad(&mut self) {
        self.tab = Tab::Launchpad;
        self.show_window();
    }

    /// 打开设置
    pub fn show_settings(&mut self) {
        self.tab = Tab::Settings;
        self.show_window();
    }

    pub fn show_window(&mut self) {
        self.visible = true;
        self.show_anim = 0.0;
        self.closing = false;
        self.was_active = false;
        self.ignore_focus_frames = 12;
        self.main_menu_open = false;
        // 「回到主页面」：关闭超过设定时长（或选择始终）后重开回第一页
        let gone = self
            .last_hidden_at
            .map(|t| t.elapsed())
            .unwrap_or(std::time::Duration::MAX);
        let should_home = match self.settings.return_home.threshold() {
            Some(th) => gone >= th,
            None => matches!(self.settings.return_home, ReturnHome::Always),
        };
        if should_home {
            self.page = 0;
        }
        crate::native::activate_app();
        crate::native::set_window_level(if self.settings.display_mode == DisplayMode::Fullscreen {
            25
        } else {
            21
        });
        // 激活之后再设一次展示选项（仅设置一次会被系统在激活时重置）
        crate::native::set_menu_bar_hidden(false);
        self.reload_wallpaper_if_changed();
        // 常驻后台期间新装的软件不会自己出现：唤出时限流重扫一次
        // （图标导出按 manifest+mtime 增量，只有布局变更才是主要代价）
        if !self.scanning
            && self
                .last_scan_done
                .map_or(true, |t| t.elapsed() >= AUTO_RESCAN_INTERVAL)
        {
            self.start_scan();
        }
        let Some(ctx) = &self.ctx else { return };
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.search.clear();
        self.status = "已就绪".into();
    }

    pub fn toggle_window(&mut self) {
        if self.visible {
            if self.tab != Tab::Launchpad {
                // 设置/卸载页打开时，热键先回到启动台
                self.tab = Tab::Launchpad;
            } else {
                self.hide_window();
            }
        } else {
            self.show_launchpad();
        }
    }

    pub fn reveal(&self, app: &AppRef) {
        let _ = Command::new("/usr/bin/open").args(["-R"]).arg(&app.path).spawn();
    }

    /// 显示「简介」窗口
    pub fn show_info(&self, app: &AppRef) {
        let path = app.path.replace('"', "\\\"");
        let script = format!(
            "tell application \"Finder\" to open information window of (POSIX file \"{path}\")"
        );
        let _ = Command::new("/usr/bin/osascript").args(["-e", &script]).output();
    }

    /// 选中当前页所有应用（⌘A）
    pub fn select_all_page(&mut self) {
        let page = self.page;
        if let Some(p) = self.layout.pages.get(page) {
            for it in &p.items {
                if let Item::App { app } = it {
                    self.selected.insert(app.bundle_id.clone());
                }
            }
        }
    }

    /// 把当前选中的应用一起拖到目标位置；压在某个 App 图标上时合并为文件夹。
    /// `t` 为目标在原始布局中的下标（由 drop_at 换算）。
    pub fn drop_selected(&mut self, to_page: usize, t: usize, allow_merge: bool) {
        if self.selected.is_empty() {
            return;
        }
        let target = self
            .layout
            .pages
            .get(to_page)
            .and_then(|p| p.items.get(t))
            .cloned();

        // 按布局顺序收集选中的 App
        let mut picked: Vec<AppRef> = Vec::new();
        for page in &self.layout.pages {
            for it in &page.items {
                if let Item::App { app } = it {
                    if self.selected.contains(&app.bundle_id) {
                        picked.push(app.clone());
                    }
                }
            }
        }
        if picked.is_empty() {
            return;
        }

        let ids: Vec<String> = picked.iter().map(|a| a.bundle_id.clone()).collect();
        for id in &ids {
            self.layout.remove_app(id);
        }

        // 拖到文件夹图标上：全部加入该文件夹
        if let Some(Item::Folder { folder }) = &target {
            if allow_merge {
                if let Some(f) = self.folder_mut(&folder.id) {
                    for a in picked {
                        if !f.apps.iter().any(|x| x.bundle_id == a.bundle_id) {
                            f.apps.push(a);
                        }
                    }
                }
                self.selected.clear();
                layout_store::normalize(&mut self.layout);
                self.clamp_page();
                let _ = layout_store::save(&self.layout);
                return;
            }
        }
        if let (Some(Item::App { app: ta }), true) = (target, allow_merge) {
            let mut apps = picked.clone();
            if !ids.contains(&ta.bundle_id) {
                apps.push(ta.clone());
            }
            let mut idx = t;
            let folder = Folder {
                id: format!("folder-{}", layout_store::now_ts()),
                name: "文件夹".into(),
                apps,
            };
            // 目标可能已被移除，重新定位插入点
            let page_len = self
                .layout
                .pages
                .get(to_page)
                .map(|p| p.items.len())
                .unwrap_or(0);
            idx = idx.min(page_len);
            self.insert_item(to_page, idx, Item::folder(folder));
        } else {
            let mut idx = t;
            for app in picked {
                let page_len = self
                    .layout
                    .pages
                    .get(to_page)
                    .map(|p| p.items.len())
                    .unwrap_or(0);
                idx = idx.min(page_len);
                self.insert_item(to_page, idx, Item::app(app));
                idx += 1;
            }
        }

        self.selected.clear();
        layout_store::normalize(&mut self.layout);
        self.clamp_page();
        let _ = layout_store::save(&self.layout);
    }

    /// 隐藏所有选中的应用
    pub fn hide_selected(&mut self) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            self.layout.hidden.insert(id.clone());
            self.layout.remove_app(&id);
            self.layout.hidden.insert(id);
        }
        self.selected.clear();
        layout_store::normalize(&mut self.layout);
        self.clamp_page();
        let _ = layout_store::save(&self.layout);
    }

    /// 把选中的应用移入废纸篓（确认后调用）
    pub fn trash_selected(&mut self) {
        let mut refs: Vec<AppRef> = Vec::new();
        for page in &self.layout.pages {
            for it in &page.items {
                for a in it.apps() {
                    if self.selected.contains(&a.bundle_id) {
                        refs.push(a);
                    }
                }
            }
        }
        if refs.is_empty() {
            return;
        }
        let paths: Vec<String> = refs
            .iter()
            .map(|a| a.path.clone())
            .collect();
        let _ok = crate::native::trash_paths(&paths);
        crate::native::play_trash_sound();
        for a in refs {
            self.layout.remove_app(&a.bundle_id);
        }
        self.selected.clear();
        self.confirm_batch = false;
        layout_store::normalize(&mut self.layout);
        self.clamp_page();
        let _ = layout_store::save(&self.layout);
        self.start_scan();
    }

    /// 用原版数据库恢复布局
    pub fn import_original_layout(&mut self) {
        if let Some(imp) = crate::orig_layout::import() {
            self.layout.pages = imp.pages;
            self.layout.hidden = imp.hidden;
            self.layout.added_at = imp.added_at;
            crate::layout_store::normalize(&mut self.layout);
            self.page = 0;
            let _ = crate::layout_store::save(&self.layout);
            self.status = "已恢复系统启动台布局".into();
        }
    }

    pub fn export_layout_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("LaunchDeck-layout.json")
            .save_file()
        {
            if let Ok(json) = serde_json::to_string_pretty(&self.layout) {
                let _ = std::fs::write(path, json);
                self.status = "布局已导出".into();
            }
        }
    }

    pub fn import_layout_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            if let Ok(txt) = std::fs::read_to_string(path) {
                if let Ok(mut l) = serde_json::from_str::<LaunchLayout>(&txt) {
                    crate::layout_store::normalize(&mut l);
                    self.layout = l;
                    let _ = crate::layout_store::save(&self.layout);
                    self.status = "布局已导入".into();
                }
            }
        }
    }

    // ---------- 隐藏 / 卸载 ----------
    pub fn toggle_hide(&mut self, app: &AppRef) {
        if self.layout.hidden.contains(&app.bundle_id) {
            self.layout.hidden.remove(&app.bundle_id);
            self.status = format!("已取消隐藏 {}", app.name);
        } else {
            self.layout.hidden.insert(app.bundle_id.clone());
            self.layout.remove_app(&app.bundle_id);
            self.layout.hidden.insert(app.bundle_id.clone()); // remove_app 会清除，重新加
            self.status = format!("已隐藏 {}", app.name);
        }
        layout_store::normalize(&mut self.layout);
        self.clamp_page();
        let _ = layout_store::save(&self.layout);
    }

    pub fn unhide(&mut self, bundle_id: &str) {
        if self.layout.hidden.remove(bundle_id) {
            if let Some(a) = self.apps.iter().find(|x| x.bundle_id == bundle_id) {
                let ar = AppRef {
                    bundle_id: if a.bundle_id.is_empty() { a.path.to_string_lossy().to_string() } else { a.bundle_id.clone() },
                    name: a.name.clone(),
                    path: a.path.to_string_lossy().to_string(),
                };
                let cap = self.layout.capacity();
                let last = self.layout.pages.len() - 1;
                if self.layout.pages[last].items.len() >= cap {
                    self.layout.pages.push(Page::default());
                }
                let last = self.layout.pages.len() - 1;
                self.layout.pages[last].items.push(Item::app(ar));
            }
            let _ = layout_store::save(&self.layout);
        }
    }

    pub fn trash(&mut self, app: &AppRef) {
        // 替身应用：只把替身文件本身移入废纸篓
        let target = self
            .apps
            .iter()
            .find(|a| a.bundle_id == app.bundle_id || a.path.to_string_lossy() == app.path)
            .and_then(|a| a.alias_path.clone())
            .unwrap_or_else(|| app.path.clone());
        let _ok = crate::native::trash_paths(&[target]);
        crate::native::play_trash_sound();
        self.layout.remove_app(&app.bundle_id);
        layout_store::normalize(&mut self.layout);
        self.clamp_page();
        let _ = layout_store::save(&self.layout);
        self.start_scan();
    }

    // ---------- 文件夹 ----------
    /// 用当前选中的应用新建文件夹（多选后右键）
    pub fn folder_from_selection(&mut self) {
        if self.selected.len() < 2 {
            return;
        }
        let sel: std::collections::HashSet<String> = self.selected.clone();
        let mut apps: Vec<crate::layout::AppRef> = Vec::new();
        let mut first_pos: Option<(usize, usize)> = None;
        for (pi, p) in self.layout.pages.iter_mut().enumerate() {
            let mut keep: Vec<Item> = Vec::with_capacity(p.items.len());
            for it in p.items.drain(..) {
                let take = matches!(&it, Item::App { app } if sel.contains(&app.bundle_id));
                if take {
                    if first_pos.is_none() {
                        first_pos = Some((pi, keep.len()));
                    }
                    apps.extend(it.apps());
                } else {
                    keep.push(it);
                }
            }
            p.items = keep;
        }
        let Some((pi, idx)) = first_pos else {
            return;
        };
        let id = format!("folder-{}", layout_store::now_ts());
        let folder = crate::layout::Folder {
            id: id.clone(),
            name: "未命名".into(),
            apps,
        };
        if let Some(p) = self.layout.pages.get_mut(pi) {
            let at = idx.min(p.items.len());
            p.items.insert(at, Item::folder(folder));
        }
        self.selected.clear();
        self.editing_folder = Some(id.clone());
        self.edit_text = "未命名".into();
        self.focus_editing = true;
        self.open_folder = Some(id);
        layout_store::normalize(&mut self.layout);
        let _ = layout_store::save(&self.layout);
    }

    pub fn create_folder(&mut self, page: usize, index: usize, with: Option<Item>) {
        let mut apps: Vec<AppRef> = Vec::new();
        // 取出原 item
        if let Some(p) = self.layout.pages.get_mut(page) {
            if index < p.items.len() {
                let it = p.items.remove(index);
                apps.extend(it.apps());
            }
        }
        if let Some(other) = with {
            apps.extend(other.apps());
        }
        let id = format!("folder-{}", layout_store::now_ts());
        let folder = Folder {
            id: id.clone(),
            name: "未命名".into(),
            apps,
        };
        if let Some(p) = self.layout.pages.get_mut(page) {
            let idx = index.min(p.items.len());
            p.items.insert(idx, Item::folder(folder));
        }
        self.editing_folder = Some(id.clone());
        self.edit_text = "未命名".into();
        self.focus_editing = true;
        self.open_folder = Some(id);
        let _ = layout_store::save(&self.layout);
    }

    pub fn rename_folder(&mut self, id: &str, name: &str) {
        for p in &mut self.layout.pages {
            for it in &mut p.items {
                if let Item::Folder { folder } = it {
                    if folder.id == id {
                        folder.name = name.to_string();
                    }
                }
            }
        }
        let _ = layout_store::save(&self.layout);
    }

    /// 文件夹内拖动排序：把 from_index 的应用移到面板落点对应的槽位
    pub fn reorder_folder(&mut self, id: &str, from_index: usize, pos: egui::Pos2, panel: egui::Rect) {
        let cols = self.layout.columns.max(1);
        let ft = panel.width() / cols as f32;
        let col = ((pos.x - panel.min.x) / ft).floor().max(0.0) as usize;
        let row = ((pos.y - (panel.min.y + 6.0)) / 178.7).floor().max(0.0) as usize;
        for p in &mut self.layout.pages {
            for it in &mut p.items {
                if let Item::Folder { folder } = it {
                    if folder.id == id && from_index < folder.apps.len() {
                        let a = folder.apps.remove(from_index);
                        let at = (row * cols + col).min(folder.apps.len());
                        folder.apps.insert(at, a);
                    }
                }
            }
        }
        let _ = layout_store::save(&self.layout);
    }

    /// 文件夹间移动：把 from 文件夹的第 index 个应用直接移入 to 文件夹
    pub fn move_app_between_folders(&mut self, from_id: &str, index: usize, to_id: &str) {
        let mut moved: Option<crate::layout::AppRef> = None;
        for p in &mut self.layout.pages {
            for it in &mut p.items {
                if let Item::Folder { folder } = it {
                    if folder.id == from_id && index < folder.apps.len() {
                        moved = Some(folder.apps.remove(index));
                    }
                }
            }
            // 源文件夹少于一应用则解散
            for it in p.items.iter_mut() {
                let mut dissolve = None;
                if let Item::Folder { folder } = it {
                    if folder.id == from_id && folder.apps.len() == 1 {
                        dissolve = Some(folder.apps[0].clone());
                    }
                }
                if let Some(a) = dissolve {
                    *it = Item::app(a);
                }
            }
        }
        if let Some(a) = moved {
            if let Some(f) = self.folder_mut(to_id) {
                if !f.apps.iter().any(|x| x.bundle_id == a.bundle_id) {
                    f.apps.push(a);
                }
            }
        }
        // 收起源文件夹
        self.folder_closing = true;
        layout_store::normalize(&mut self.layout);
        self.clamp_page();
        let _ = layout_store::save(&self.layout);
    }

    /// 从文件夹里拖出应用：放到当前页网格上，并把文件夹收起（不是消失）
    pub fn drag_app_out_of_folder(&mut self, id: &str, index: usize, drop: Option<egui::Pos2>) {
        let mut moved: Option<crate::layout::AppRef> = None;
        for p in &mut self.layout.pages {
            for it in &mut p.items {
                if let Item::Folder { folder } = it {
                    if folder.id == id && index < folder.apps.len() {
                        moved = Some(folder.apps.remove(index));
                    }
                }
            }
            for it in p.items.iter_mut() {
                let mut dissolve = None;
                if let Item::Folder { folder } = it {
                    if folder.apps.len() == 1 {
                        dissolve = Some(folder.apps[0].clone());
                    }
                }
                if let Some(a) = dissolve {
                    *it = Item::app(a);
                }
            }
        }
        if let Some(a) = moved {
            let page_idx = self.page.min(self.layout.pages.len().saturating_sub(1));
            // 按落点算目标格子（落在网格外就放到末尾）
            let mut slot: Option<usize> = None;
            if let (Some(pos), Some(size)) = (drop, self.screen_size) {
                let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), size);
                let geom = crate::ui::Geom::new(rect, self.layout.columns, self.layout.rows);
                slot = geom.cell_at(pos);
            }
            if let Some(page) = self.layout.pages.get_mut(page_idx) {
                let at = slot.unwrap_or(page.items.len()).min(page.items.len());
                page.items.insert(at, Item::app(a));
            }
        }
        // 自动收起文件夹（走关闭动画，不是硬切）
        self.folder_closing = true;
        layout_store::normalize(&mut self.layout);
        let _ = layout_store::save(&self.layout);
    }

    /// 从文件夹里移除应用；只剩一个应用时自动解散文件夹
    pub fn remove_from_folder(&mut self, id: &str, index: usize) {
        for p in &mut self.layout.pages {
            for it in &mut p.items {
                if let Item::Folder { folder } = it {
                    if folder.id == id && index < folder.apps.len() {
                        folder.apps.remove(index);
                    }
                }
            }
            for it in p.items.iter_mut() {
                let mut dissolve = None;
                if let Item::Folder { folder } = it {
                    if folder.apps.len() == 1 {
                        dissolve = Some(folder.apps[0].clone());
                    }
                }
                if let Some(a) = dissolve {
                    *it = Item::app(a);
                }
            }
        }
        layout_store::normalize(&mut self.layout);
        let _ = layout_store::save(&self.layout);
    }

    pub fn ungroup(&mut self, id: &str) {
        for p in &mut self.layout.pages {
            let mut add: Vec<Item> = Vec::new();
            p.items.retain_mut(|it| {
                if let Item::Folder { folder } = it {
                    if folder.id == id {
                        add = folder.apps.iter().cloned().map(Item::app).collect();
                        return false;
                    }
                }
                true
            });
            p.items.extend(add);
        }
        layout_store::normalize(&mut self.layout);
        let _ = layout_store::save(&self.layout);
    }

    // ---------- 拖拽 ----------
    pub fn begin_drag(&mut self, item: Item, page: usize, index: usize) {
        self.multi_drag =
            self.selected.len() > 1 && self.selected.contains(item.bundle_id());
        self.drag = Some(DragState {
            item,
            from_page: page,
            from_index: index,
            from_folder: None,
        });
    }

    /// 放置到目标位置。`slot` 是目标页的可见格下标（拖拽预览布局，
    /// 源图标已抽走、后续格子前移；跨页时即目标页原始下标）：
    /// - 松手时压在文件夹图标上 → 加入该文件夹
    /// - 松手时压在应用图标上 → 合并成新文件夹
    /// - 其余情况普通移动（落到 slot 所在格）
    pub fn drop_at(&mut self, to_page: usize, slot: usize, on_icon: bool) {
        let Some(drag) = self.drag.take() else { return };
        let same_page0 = drag.from_page == to_page;
        if self.multi_drag {
            self.multi_drag = false;
            // 多选：换算原始下标后交给 drop_selected
            let t = if same_page0 && slot >= drag.from_index {
                slot + 1
            } else {
                slot
            };
            self.drop_selected(to_page, t, on_icon);
            return;
        }
        let src_id = drag.item.bundle_id().to_string();
        let same_page = same_page0;
        // 放回原位（未压在图标上）= 无操作。注意：源图标被抽走后，右邻会
        // 滑进源格子（slot == from_index），此时压在图标上是对右邻操作，
        // 不能当作放回原位——这正是「向右拖到紧邻图标不生效」的根源
        if same_page && drag.from_index == slot && !on_icon {
            return;
        }
        // 可见格 slot 在原始布局中的下标（同页且在源之后 → 原始下标 +1）
        let t = if same_page && slot >= drag.from_index {
            slot + 1
        } else {
            slot
        };
        let target = self
            .layout
            .pages
            .get(to_page)
            .and_then(|p| p.items.get(t))
            .cloned();

        match target {
            // 拖进文件夹（必须明确压在文件夹图标上）
            Some(Item::Folder { folder }) if on_icon && folder.id != src_id => {
                self.remove_item_at(drag.from_page, drag.from_index);
                if let Some(f) = self.folder_mut(&folder.id) {
                    for a in drag.item.apps() {
                        if !f.apps.iter().any(|x| x.bundle_id == a.bundle_id) {
                            f.apps.push(a);
                        }
                    }
                }
            }
            // 两个 App 合并成文件夹（必须明确压在目标 App 图标上）
            Some(Item::App { app: ta })
                if on_icon
                    && matches!(drag.item, Item::App { .. })
                    && ta.bundle_id != src_id =>
            {
                let mut apps = drag.item.apps();
                apps.extend(ta_apps(&self.layout, to_page, t));
                // 同一页内向前拖动时，移除来源后目标下标会左移
                let target_idx = if same_page && drag.from_index < t {
                    t - 1
                } else {
                    t
                };
                self.remove_item_at(drag.from_page, drag.from_index);
                self.remove_item_at(to_page, target_idx);
                self.insert_item(to_page, target_idx, Item::folder(Folder {
                    id: format!("folder-{}", layout_store::now_ts()),
                    name: "未命名".into(),
                    apps,
                }));
                let id = self
                    .layout
                    .pages
                    .get(to_page)
                    .and_then(|p| p.items.get(target_idx))
                    .and_then(|it| match it {
                        Item::Folder { folder } => Some(folder.id.clone()),
                        _ => None,
                    });
                if let Some(id) = id {
                    self.editing_folder = Some(id.clone());
                    self.edit_text = "未命名".into();
                    self.focus_editing = true;
                    self.open_folder = Some(id);
                }
            }
            // 普通移动：直接落到可见格 slot
            _ => {
                self.remove_item_at(drag.from_page, drag.from_index);
                self.insert_item(to_page, slot, drag.item);
            }
        }

        layout_store::normalize(&mut self.layout);
        self.clamp_page();
        let _ = layout_store::save(&self.layout);
    }

    fn remove_item_at(&mut self, page: usize, index: usize) -> Option<Item> {
        let p = self.layout.pages.get_mut(page)?;
        if index < p.items.len() {
            Some(p.items.remove(index))
        } else {
            None
        }
    }

    fn insert_item(&mut self, page: usize, index: usize, item: Item) {
        while self.layout.pages.len() <= page {
            self.layout.pages.push(Page::default());
        }
        let cap = self.layout.capacity();
        let p = &mut self.layout.pages[page];
        let idx = index.min(p.items.len()).min(cap.saturating_sub(1));
        p.items.insert(idx, item);
    }

    fn folder_mut(&mut self, id: &str) -> Option<&mut Folder> {
        for p in &mut self.layout.pages {
            for it in &mut p.items {
                if let Item::Folder { folder } = it {
                    if folder.id == id {
                        return Some(folder);
                    }
                }
            }
        }
        None
    }

    /// 立即切换窗口/全屏，并持久化。
    ///
    /// 原版的全屏是「无边框窗口铺满屏幕」而非 macOS 原生全屏，这样 Dock 仍然可见，
    /// 观感与系统启动台一致。
    pub fn set_display_mode(&mut self, mode: DisplayMode) {
        self.settings.display_mode = mode;
        if let Some(ctx) = &self.ctx {
            Self::apply_display_mode(ctx, mode, self.settings.show_on);
        }
        let _ = launchdeck_core::save_settings(&self.settings);
    }

    /// 图标样式枚举 → 资源文件下标（style0..3.png）
    pub fn icon_index(style: launchdeck_core::IconStyle) -> usize {
        match style {
            launchdeck_core::IconStyle::Rounded => 0,
            launchdeck_core::IconStyle::Circle => 1,
            launchdeck_core::IconStyle::Square => 2,
            launchdeck_core::IconStyle::Custom => 3,
        }
    }

    pub fn apply_display_mode(ctx: &egui::Context, mode: DisplayMode, on: ScreenChoice) {
        crate::native::set_menu_bar_hidden(false);
        crate::native::set_window_level(if mode == DisplayMode::Fullscreen { 25 } else { 21 });
        let frame = crate::native::screen_frame(on);
        let screen = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(2048.0, 1152.0));
        if mode == DisplayMode::Fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
            if let Some((x, y, w, h)) = frame {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    w as f32, h as f32,
                )));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                    x as f32, y as f32,
                )));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(screen));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(0.0, 0.0)));
            }
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            // 原版窗口模式是无边框的整块毛玻璃，不要系统标题栏
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
            let size = egui::vec2(1200.0, 800.0);
            let pos = match frame {
                Some((x, y, w, h)) => egui::pos2(
                    x as f32 + (w as f32 - size.x) / 2.0,
                    y as f32 + (h as f32 - size.y - 28.0) / 2.0,
                ),
                None => egui::pos2(
                    (screen.x - size.x) / 2.0,
                    (screen.y - size.y - 28.0) / 2.0,
                ),
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        }
    }

    pub fn reorganize(&mut self) {
        layout_store::apply_sort(&mut self.layout, &self.settings);
        self.clamp_page();
        let _ = layout_store::save(&self.layout);
        self.status = "已重新整理布局".into();
    }

    /// 整理当前页：把后面页面的应用往前搬，填满当前页的空位
    pub fn reorganize_current_page(&mut self) {
        layout_store::fill_page_from_later(&mut self.layout, self.page);
        self.clamp_page();
        let _ = layout_store::save(&self.layout);
        self.status = "已整理当前页空位".into();
    }

    /// 收拢所选页面（「整理布局」弹窗里的多选）
    pub fn reorganize_selected_pages(&mut self) {
        let sel: Vec<usize> = self.tidy_selected.iter().copied().collect();
        if sel.is_empty() {
            return;
        }
        layout_store::compact_pages(&mut self.layout, &sel);
        self.clamp_page();
        self.tidy_selected.clear();
        self.tidy_popup_open = false;
        let _ = layout_store::save(&self.layout);
        self.status = "已整理所选页面".into();
    }

    /// 把应用添加到程序坞（写 dock.plist + 重启 Dock）
    pub fn add_to_dock(&mut self, app: &AppRef) {
        let raw = app.path.trim_end_matches('/');
        let stem = raw.rsplit('/').next().unwrap_or(raw);
        let existing = Command::new("/usr/bin/defaults")
            .args(["read", "com.apple.dock", "persistent-apps"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        if existing.contains(stem) {
            self.status = format!("「{}」已在程序坞中", app.name);
            return;
        }
        let url = format!("file://{}/", encode_url_path(raw));
        let tile = format!(
            "<dict><key>tile-data</key><dict><key>file-data</key><dict><key>_CFURLString</key><string>{url}</string><key>_CFURLStringType</key><integer>15</integer></dict></dict></dict>"
        );
        let ok = Command::new("/usr/bin/defaults")
            .args(["write", "com.apple.dock", "persistent-apps", "-array-add", &tile])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            let _ = Command::new("/usr/bin/killall").arg("Dock").output();
            self.status = format!("已把「{}」添加到程序坞", app.name);
        } else {
            self.status = "添加到程序坞失败".into();
        }
    }

    // ---------- 类型筛选 ----------

    pub fn set_category_filter(&mut self, raw: Option<String>) {
        self.category_filter = raw;
        self.search.clear();
        self.page = 0;
    }

    /// LSApplicationCategoryType → 中文名
    pub fn category_label(raw: &str) -> &'static str {
        let suffix = raw.strip_prefix("public.app-category.").unwrap_or(raw);
        match suffix {
            "games" => "游戏",
            "productivity" => "效率",
            "utilities" => "实用工具",
            "social-networking" => "社交",
            "entertainment" => "娱乐",
            "business" => "商务",
            "education" => "教育",
            "finance" => "财务",
            "healthcare-fitness" => "健康与健身",
            "graphics-design" => "图形与设计",
            "developer-tools" => "开发工具",
            "music" => "音乐",
            "news" => "新闻",
            "books" => "图书",
            "food-drink" => "美食佳饮",
            "reference" => "参考",
            "travel" => "旅行",
            "weather" => "天气",
            "sports" => "体育",
            "photography" => "摄影",
            "video" => "视频",
            "lifestyle" => "生活方式",
            "medical" => "医疗",
            "navigation" => "导航",
            "catalogs" => "目录",
            s if s.contains("games") => "游戏",
            _ => "其他",
        }
    }

    /// 当前扫描结果中出现过的分类（原始值, 中文名）
    pub fn category_options(&self) -> Vec<(String, &'static str)> {
        let mut out: Vec<(String, &'static str)> = Vec::new();
        for a in &self.apps {
            if let Some(c) = &a.category {
                if !out.iter().any(|(r, _)| r == c) {
                    out.push((c.clone(), Self::category_label(c)));
                }
            }
        }
        out.sort_by(|x, y| x.1.cmp(y.1));
        out
    }

    /// 类型筛选后的应用（保持布局顺序，只含 App 项）
    pub fn filtered_items(&self) -> Vec<Item> {
        let Some(f) = &self.category_filter else { return Vec::new() };
        let mut out = Vec::new();
        for page in &self.layout.pages {
            for it in &page.items {
                if let Item::App { app } = it {
                    let cat = self
                        .apps
                        .iter()
                        .find(|a| {
                            (!a.bundle_id.is_empty() && a.bundle_id == app.bundle_id)
                                || a.path.to_string_lossy() == app.path
                        })
                        .and_then(|a| a.category.clone());
                    if cat.as_deref() == Some(f.as_str()) {
                        out.push(it.clone());
                    }
                }
            }
        }
        out
    }

    /// 提交应用重命名（右键「重命名」）
    pub fn commit_app_rename(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let Some(bid) = self.renaming_app.clone() else { return };
        for page in &mut self.layout.pages {
            for it in &mut page.items {
                if let Item::App { app } = it {
                    if app.bundle_id == bid || app.path == bid {
                        app.name = name.to_string();
                    }
                }
            }
        }
        let _ = layout_store::save(&self.layout);
    }

    // ---------- 深度卸载 ----------    /// 打开深度卸载确认面板（扫描残留 + 检测运行状态）
    pub fn open_uninstall_detail(&mut self, bid: &str) {
        let Some(a) = self
            .apps
            .iter()
            .find(|a| a.bundle_id == bid || a.path.to_string_lossy() == bid)
            .cloned()
        else {
            return;
        };
        let path = a
            .alias_path
            .clone()
            .unwrap_or_else(|| a.path.to_string_lossy().to_string());
        let residues = if a.is_alias {
            Vec::new()
        } else {
            launchdeck_core::uninstall::scan(&a.bundle_id, &a.name, &a.path)
        };
        let running = !a.is_alias && crate::native::app_running_at(&a.path);
        let n = residues.len();
        self.uninstall_detail = Some(UninstallDetail {
            bid: bid.to_string(),
            name: a.name.clone(),
            path,
            residues,
            checked: vec![true; n],
            running,
            attempts: 0,
        });
    }

    /// 应用正在运行时，先退出再继续卸载（第一次正常退出，再次点击强制退出）
    pub fn quit_running_app(&mut self) {
        let Some(d) = &mut self.uninstall_detail else { return };
        d.attempts = d.attempts.saturating_add(1);
        let force = d.attempts > 1;
        let bid = d.bid.clone();
        if let Some(a) = self
            .apps
            .iter()
            .find(|a| a.bundle_id == bid || a.path.to_string_lossy() == bid)
        {
            let path = a
                .alias_path
                .as_ref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| a.path.clone());
            crate::native::terminate_app_at(&path, force);
        }
        // 给应用一点退出时间，随后重查状态
        std::thread::sleep(std::time::Duration::from_millis(800));
        if let Some(d) = &mut self.uninstall_detail {
            d.running = self
                .apps
                .iter()
                .find(|a| a.bundle_id == d.bid || a.path.to_string_lossy() == d.bid)
                .map(|a| crate::native::app_running_at(&a.path))
                .unwrap_or(false);
        }
    }

    /// 按勾选把应用与残留移入废纸篓
    pub fn confirm_uninstall_detail(&mut self) {
        let Some(d) = self.uninstall_detail.take() else { return };
        let mut paths: Vec<String> = Vec::new();
        if d.residues.is_empty() {
            paths.push(d.path.clone());
        } else {
            for (r, ok) in d.residues.iter().zip(&d.checked) {
                if *ok {
                    for p in &r.paths {
                        paths.push(p.to_string_lossy().to_string());
                    }
                }
            }
        }
        if !paths.is_empty() {
            let _ok = crate::native::trash_paths(&paths);
            crate::native::play_trash_sound();
        }
        // 应用本体被删（或没有残留明细）才从布局移除
        let app_gone = d.residues.is_empty()
            || d
                .residues
                .iter()
                .zip(&d.checked)
                .any(|(r, ok)| *ok && r.kind == launchdeck_core::uninstall::ResidueKind::AppFiles);
        if app_gone {
            self.layout.remove_app(&d.bid);
            layout_store::normalize(&mut self.layout);
            self.clamp_page();
            let _ = layout_store::save(&self.layout);
        }
        self.start_scan();
    }

    // ---------- 更新检查 ----------

    pub fn spawn_update_check(&mut self) {
        if self.update_rx.is_some() {
            return;
        }
        self.update_status = "正在检查更新…".into();
        let (tx, rx) = channel();
        self.update_rx = Some(rx);
        thread::spawn(move || {
            let _ = tx.send(check_latest_release());
        });
    }

    fn poll_update(&mut self) {
        if let Some(rx) = &self.update_rx {
            if let Ok(info) = rx.try_recv() {
                self.update_rx = None;
                match info {
                    Some(info) => {
                        self.update_status = format!("发现新版本 {}", info.tag);
                        if self.settings.auto_download {
                            let url = info.url.clone();
                            if !url.is_empty() {
                                let _ = Command::new("/usr/bin/open").arg(&url).spawn();
                            }
                        }
                        self.update_info = Some(info);
                    }
                    None => self.update_status = "已是最新版本".into(),
                }
            }
        }
    }
}

impl eframe::App for LaunchDeckApp {
    /// 透明窗口：设置/卸载页只画各自的浮层，四周透出真实桌面
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // e2e 模式：窗口隐藏时 egui 会停帧，指令/状态文件就没人处理了，持续请求重绘
        #[cfg(feature = "e2e")]
        if std::env::var("LAUNCHDECK_E2E").is_ok() {
            ctx.request_repaint();
        }
        self.ctx = Some(ctx.clone());
        self.screen_size = Some(ctx.screen_rect().size());

        // 设置是独立的悬浮窗：进入设置时把主窗口缩成 400×高度，退出时恢复全屏
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(2048.0, 1152.0));
        let set_h = crate::ui::TAB_H[self.settings_section.min(4)];
        let entering_settings = self.tab == Tab::Settings && self.prev_tab != Tab::Settings;
        let section_changed = self.tab == Tab::Settings && self.prev_section != self.settings_section;
        let leaving_settings = self.tab != Tab::Settings && self.prev_tab == Tab::Settings;
        if entering_settings || section_changed {
            let size = egui::vec2(crate::ui::SET_W, set_h);
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                (monitor.x - size.x) / 2.0,
                (monitor.y - size.y) / 2.0,
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            // 设置窗口用普通层级（不再悬浮在所有 App 之上），但打开时把自己激活到前台
            crate::native::set_window_level(0);
            crate::native::activate_self();
            self.ignore_focus_frames = 6;
            self.reposition = 0;
        }
        if leaving_settings {
            Self::apply_display_mode(ctx, self.settings.display_mode, self.settings.show_on);
            self.reposition = 8; // 恢复全屏后重新贴到 (0,0)，否则可能被菜单栏挤到 y=30
            self.ignore_focus_frames = 6;
        }
        self.prev_tab = self.tab;
        self.prev_section = self.settings_section;


        if self.first_frame {
            self.first_frame = false;
            // 直接以设置页启动时不要再套全屏尺寸，否则会覆盖上面的 400×610
            if self.tab != Tab::Settings {
                Self::apply_display_mode(ctx, self.settings.display_mode, self.settings.show_on);
            }
            self.reposition = 6;
        }

        // 无边框窗口在首帧可能被系统压在菜单栏下方，多校正几帧
        if self.reposition > 0 && self.tab != Tab::Settings {
            self.reposition -= 1;
            if self.settings.display_mode == DisplayMode::Fullscreen {
                if let Some((x, y, w, h)) = crate::native::screen_frame(self.settings.show_on) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                        x as f32, y as f32,
                    )));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        w as f32, h as f32,
                    )));
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                        0.0, 0.0,
                    )));
                    if let Some(size) = ctx.input(|i| i.viewport().monitor_size) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                    }
                }
                crate::native::set_menu_bar_hidden(false);
            }
        }

        if !self.native_ready {
            self.native_ready = true;
            native::register_f4();
            native::install_swipe_tap();
            native::install_pinch_monitor();
            native::set_pinch_fingers(match self.settings.pinch_gesture {
                PinchGesture::Off => 0,
                PinchGesture::Three => 3,
                PinchGesture::FourOrFive => 4,
                PinchGesture::Five => 5,
            });
            native::install_reopen_handler();
        }

        // 捏合手势：捏合打开，张开关闭
        if self.settings.pinch_gesture != PinchGesture::Off {
            match native::take_pinch() {
                1 => {
                    if !self.visible || self.closing {
                        self.show_launchpad();
                    }
                }
                -1 => {
                    if self.visible
                        && !self.closing
                        && self.tab == Tab::Launchpad
                        && self.open_folder.is_none()
                    {
                        self.hide_window();
                    }
                }
                _ => {}
            }
        }

        // App 构造阶段设置的应用图标不会被 Dock 采纳（NSApp 尚未完成启动），
        // 事件循环跑起来后再补设几次，Dock 才会换成我们自己的图标。
        if self.icon_apply_frames < 40 {
            self.icon_apply_frames += 1;
            if matches!(self.icon_apply_frames, 2 | 12 | 40) {
                native::set_app_icon(Self::icon_index(self.settings.icon_style));
            }
        }

        if native::take_hotkey() || native::take_corner() || native::take_f4() {
            if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
                eprintln!("[app] hotkey -> toggle_window (visible={})", self.visible);
            }
            self.toggle_window();
            if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
                eprintln!("[app] toggle done (visible={}, closing={})", self.visible, self.closing);
            }
        }

        if self.visible && self.tab == Tab::Launchpad {
            let d = native::take_swipe();
            if d != 0 {
                let n = self.layout.page_count().max(1) as i32;
                let target = (self.page as i32 + d).clamp(0, n - 1) as usize;
                if target != self.page {
                    self.set_page(target, ctx);
                }
            }
        }

        match native::take_menu_action() {
            native::MENU_OPEN => self.show_launchpad(),
            native::MENU_SETTINGS => self.show_settings(),
            native::MENU_QUIT => std::process::exit(0),
            native::MENU_FULLSCREEN => self.set_display_mode(DisplayMode::Fullscreen),
            native::MENU_WINDOW => self.set_display_mode(DisplayMode::Window),
            native::MENU_REFRESH => self.start_scan(),
            _ => {}
        }

        if self.applied_theme.is_none() {
            self.applied_theme = Some(Theme::Dark);
            // 原版界面始终为深色
            ctx.set_visuals(egui::Visuals::dark());
        }

        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        if self.closing {
            self.show_anim -= dt / 0.24;
            if self.show_anim <= 0.0 {
                self.finish_hide(ctx);
            } else {
                ctx.request_repaint();
            }
        } else if self.visible && self.show_anim < 1.0 {
            self.show_anim = (self.show_anim + dt / 0.36).min(1.0);
            ctx.request_repaint();
        }
        if self.page_anim < 1.0 {
            self.page_anim = (self.page_anim + dt / 0.24).min(1.0);
            ctx.request_repaint();
        }

        self.poll_scan();
        self.poll_update();
        self.poll_wallpaper(ctx);
        if self.scanning {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        // e2e 钩子在绘制前执行：指令当帧生效，保持型状态不会被绘制内的
        // tick_folder_hover 清掉
        crate::e2e::tick(self);
        self.handle_keys(ctx);
        ui::draw(self, ctx);

        // 切换应用（窗口失焦）时自动收起启动台，与原版一致；
        // 放在绘制之后判断，避免“点击按钮那一帧”还没切页就被误收。
        if self.ignore_focus_frames > 0 {
            self.ignore_focus_frames -= 1;
        } else if self.visible && self.tab == Tab::Launchpad {
            let focused = crate::native::frontmost_is_self()
                || ctx.input(|i| i.viewport().focused).unwrap_or(false) && crate::native::app_is_active();
            if focused {
                self.was_active = true;
                self.unfocused_frames = 0;
            } else if self.was_active {
                self.unfocused_frames = self.unfocused_frames.saturating_add(1);
                if self.unfocused_frames >= 3 {
                    self.hide_window();
                    self.unfocused_frames = 0;
                    self.was_active = false;
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        } else {
            self.unfocused_frames = 0;
        }
    }
}

impl LaunchDeckApp {
    fn handle_keys(&mut self, ctx: &egui::Context) {
        let focus_none = ctx.memory(|m| m.focused().is_none());

        // Esc：依次关闭弹窗 → 文件夹 → 清空搜索 → 清空选择
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.confirm_delete.is_some() {
                self.confirm_delete = None;
            } else if self.tab != Tab::Launchpad {
                self.tab = Tab::Launchpad;
            } else if self.open_folder.is_some() {
                self.close_folder();
                self.editing_folder = None;
            } else if self.main_menu_open {
                self.main_menu_open = false;
            } else if self.tidy_popup_open {
                self.tidy_popup_open = false;
            } else if self.category_filter.is_some() {
                self.category_filter = None;
            } else if !self.search.is_empty() {
                self.search.clear();
            } else if !self.selected.is_empty() {
                self.selected.clear();
            } else if self.settings.display_mode == DisplayMode::Fullscreen
                && self.tab == Tab::Launchpad
            {
                self.hide_window();
            }
        }

        // Cmd+A 全选当前页
        if ctx.input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::A))
            && self.tab == Tab::Launchpad
            && self.search.is_empty()
        {
            self.select_all_page();
        }

        // Cmd+, 打开设置（macOS 习惯）
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Comma)) {
            self.tab = Tab::Settings;
            self.search.clear();
            return;
        }

        if self.tab != Tab::Launchpad || self.open_folder.is_some() {
            return;
        }

        // 输入即搜索（无焦点时）
        if focus_none && self.search.is_empty() {
            let typed: String = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect()
            });
            let typed = typed.replace(|c: char| c.is_control(), "");
            if !typed.is_empty() {
                self.search.push_str(&typed);
                self.search_focus = true;
            }
        }

        if !self.search.is_empty() {
            return;
        }

        // 翻页（方向键 / 触控板横扫）
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.next_page(ctx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            self.prev_page(ctx);
        }
        let dx = ctx.input(|i| i.raw_scroll_delta.x);
        if dx.abs() > 0.5 {
            self.scroll_accum += dx;
            if self.scroll_accum < -80.0 {
                self.next_page(ctx);
                self.scroll_accum = 0.0;
            } else if self.scroll_accum > 80.0 {
                self.prev_page(ctx);
                self.scroll_accum = 0.0;
            }
        } else {
            self.scroll_accum *= 0.9;
        }
    }

    pub fn next_page(&mut self, ctx: &egui::Context) {
        if self.page + 1 < self.layout.page_count() {
            self.set_page(self.page + 1, ctx);
        }
    }

    pub fn prev_page(&mut self, ctx: &egui::Context) {
        self.set_page(self.page.saturating_sub(1), ctx);
    }

    /// 带动画切换页面
    pub fn set_page(&mut self, target: usize, ctx: &egui::Context) {
        if target == self.page || target >= self.layout.page_count() {
            return;
        }
        self.page_dir = if target > self.page { 1.0 } else { -1.0 };
        self.page_prev = self.page;
        self.page_anim = 0.0;
        self.page = target;
        ctx.request_repaint();
    }

    /// 播放文件夹收起动画
    pub fn close_folder(&mut self) {
        if self.open_folder.is_some() {
            self.folder_closing = true;
        }
    }
}

fn ta_apps(layout: &LaunchLayout, page: usize, index: usize) -> Vec<AppRef> {
    layout
        .pages
        .get(page)
        .and_then(|p| p.items.get(index))
        .map(|it| it.apps())
        .unwrap_or_default()
}

/// 解析替身：可解析的换成目标路径（记录替身路径用于卸载），
/// 解析不了或与真实条目重复的丢弃
pub fn normalize_scan(list: Vec<AppInfo>) -> Vec<AppInfo> {
    let mut list: Vec<AppInfo> = list
        .into_iter()
        .filter_map(|mut a| {
            if !a.is_alias {
                return Some(a);
            }
            let resolved = resolve_alias(&a.path)?;
            a.alias_path = Some(a.path.to_string_lossy().to_string());
            a.path = std::path::PathBuf::from(&resolved);
            a.id = resolved;
            a.bundle_id = launchdeck_core::AppScanner::read_bundle_id(&a.path).unwrap_or_default();
            Some(a)
        })
        .collect();
    let mut seen: HashSet<String> = list
        .iter()
        .filter(|a| !a.is_alias)
        .map(|a| {
            if a.bundle_id.is_empty() {
                a.id.clone()
            } else {
                a.bundle_id.clone()
            }
        })
        .collect();
    list.retain(|a| {
        if !a.is_alias {
            return true;
        }
        let key = if a.bundle_id.is_empty() {
            a.id.clone()
        } else {
            a.bundle_id.clone()
        };
        seen.insert(key)
    });
    list
}

/// 解析替身/符号链接指向的真实 .app 路径；不是替身或解析失败返回 None
fn resolve_alias(p: &std::path::Path) -> Option<String> {
    // 符号链接：canonicalize 直接给出目标
    if let Ok(t) = std::fs::canonicalize(p) {
        if t != p && t.is_dir() {
            return Some(t.to_string_lossy().to_string());
        }
        if t != p {
            return None;
        }
    }
    // Finder 替身文件
    let script = format!(
        "tell application \"Finder\" to get POSIX path of (original item of (POSIX file \"{}\" as alias))",
        p.display()
    );
    let out = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout)
        .trim()
        .trim_matches('"')
        .to_string();
    if s.is_empty() || !std::path::Path::new(&s).is_dir() {
        return None;
    }
    Some(s)
}

/// 路径 → URL path（百分号编码，用于 dock.plist 的 file:// URL）
fn encode_url_path(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'_' | b'~' | b'-' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

const UPDATE_API: &str = "https://api.github.com/repos/gree-cs/LaunchDeck/releases/latest";

/// 查询 GitHub 最新 Release；有比当前版本更新的返回 Some
fn check_latest_release() -> Option<UpdateInfo> {
    let out = Command::new("/usr/bin/curl")
        .args([
            "-sfSL",
            "--max-time",
            "15",
            "-H",
            "User-Agent: LaunchDeck",
            "-H",
            "Accept: application/vnd.github+json",
            UPDATE_API,
        ])
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let tag = v.get("tag_name")?.as_str()?.to_string();
    if !is_newer(&tag, env!("CARGO_PKG_VERSION")) {
        return None;
    }
    let url = v
        .get("html_url")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    let notes = v
        .get("body")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .chars()
        .take(400)
        .collect();
    Some(UpdateInfo { tag, url, notes })
}

/// 宽松的 semver 比较（tag 可以带 v 前缀/多出来的段）
fn is_newer(tag: &str, cur: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches(|c: char| !c.is_ascii_digit())
            .split('.')
            .map(|p| {
                p.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    };
    let (a, b) = (parse(tag), parse(cur));
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// 生成一张柔和的径向渐变纹理，用于背景光晕
fn make_glow(ctx: &egui::Context) -> egui::TextureHandle {
    let size = 256usize;
    let mut pixels = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 - size as f32 / 2.0) / (size as f32 / 2.0);
            let dy = (y as f32 - size as f32 / 2.0) / (size as f32 / 2.0);
            let d = (dx * dx + dy * dy).sqrt().min(1.0);
            let a = ((1.0 - d).powi(2) * 70.0) as u8;
            pixels.extend_from_slice(&[120u8, 155, 235, a]);
        }
    }
    let img = egui::ColorImage::from_rgba_unmultiplied([size, size], &pixels);
    ctx.load_texture("bg-glow", img, egui::TextureOptions::LINEAR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1", "0.1.2"));
        assert!(is_newer("2.0", "1.9.9"));
    }

    #[test]
    fn url_path_encoding() {
        assert_eq!(encode_url_path("/Applications/My App.app"), "/Applications/My%20App.app");
        assert_eq!(encode_url_path("/Applications/微信.app"), "/Applications/%E5%BE%AE%E4%BF%A1.app");
        assert_eq!(encode_url_path("/a~b-c_d.e"), "/a~b-c_d.e");
    }

    #[test]
    fn category_labels() {
        assert_eq!(LaunchDeckApp::category_label("public.app-category.games"), "游戏");
        assert_eq!(LaunchDeckApp::category_label("public.app-category.productivity"), "效率");
        assert_eq!(LaunchDeckApp::category_label("public.app-category.developer-tools"), "开发工具");
        assert_eq!(LaunchDeckApp::category_label("weird-value"), "其他");
    }

    #[test]
    fn resolve_alias_follows_symlink() {
        let target = "/System/Applications/Calculator.app";
        if !std::path::Path::new(target).exists() {
            return;
        }
        let link = std::env::temp_dir().join("LaunchDeckAliasResolve.app");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(target, &link).unwrap();
        assert_eq!(resolve_alias(&link).as_deref(), Some(target));
        let _ = std::fs::remove_file(&link);
    }

    #[test]
    fn normalize_scan_resolves_and_dedups_alias() {
        let mk = |id: &str, alias: bool| AppInfo {
            id: format!("/x/{id}.app"),
            name: id.into(),
            path: format!("/x/{id}.app").into(),
            version: "1".into(),
            bundle_id: format!("bid.{id}"),
            icon_path: None,
            category: None,
            size: 1,
            modified: std::time::SystemTime::UNIX_EPOCH,
            is_hidden: false,
            is_system: false,
            is_alias: alias,
            alias_path: None,
        };
        let list = vec![mk("real", false), mk("dupe", true), mk("keep", true)];
        // "dupe"/"keep" 是替身但目标无法解析（路径不存在）→ 应被丢弃
        let out = normalize_scan(list);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "real");
    }

    #[test]
    fn return_home_threshold_matches_labels() {
        use launchdeck_core::ReturnHome;
        assert_eq!(ReturnHome::Always.threshold(), None);
        assert_eq!(ReturnHome::Never.threshold(), None);
        assert_eq!(ReturnHome::FiveSeconds.threshold(), Some(std::time::Duration::from_secs(5)));
        assert_eq!(ReturnHome::FiveMinutes.threshold(), Some(std::time::Duration::from_secs(300)));
    }
}

#[cfg(test)]
mod drop_tests {
    use super::*;
    use crate::layout::*;

    fn setup(items: Vec<Item>) -> (LaunchDeckApp, LaunchLayout) {
        let mut app = LaunchDeckApp::new_headless();
        let mut l = LaunchLayout { columns: 7, rows: 5, ..Default::default() };
        l.pages[0].items = items;
        app.layout = l.clone();
        (app, l)
    }

    fn app_item(id: char) -> Item {
        Item::app(AppRef {
            bundle_id: format!("bid.{id}"),
            name: id.to_string(),
            path: format!("/{id}"),
        })
    }

    #[test]
    fn drop_left_and_right_are_symmetric() {
        // [a b c d e] 把 b(from=1) 向左拖到可见格 0 / 把 d(from=3) 向左拖到可见格 1
        // 以及向右拖：a(from=0)→可见格2、b(from=1)→可见格3
        let mk = || vec![app_item('a'), app_item('b'), app_item('c'), app_item('d'), app_item('e')];
        let ids = |l: &LaunchLayout| -> Vec<char> {
            l.pages[0].items.iter().map(|i| i.bundle_id().chars().last().unwrap()).collect()
        };

        // 向右一格：a(0) 拖到可见格 2（c 的位置）→ [b c a d e]
        let (mut app, _) = setup(mk());
        app.begin_drag(app_item('a'), 0, 0);
        app.drop_at(0, 2, false);
        assert_eq!(ids(&app.layout), vec!['b', 'c', 'a', 'd', 'e'], "向右移动一格");

        // 向右两格：a(0) 拖到可见格 3 → [b c d a e]
        let (mut app, _) = setup(mk());
        app.begin_drag(app_item('a'), 0, 0);
        app.drop_at(0, 3, false);
        assert_eq!(ids(&app.layout), vec!['b', 'c', 'd', 'a', 'e'], "向右移动两格");

        // 向左一格：d(3) 拖到可见格 1（b 的位置）→ [a d b c e]
        let (mut app, _) = setup(mk());
        app.begin_drag(app_item('d'), 0, 3);
        app.drop_at(0, 1, false);
        assert_eq!(ids(&app.layout), vec!['a', 'd', 'b', 'c', 'e'], "向左移动一格");

        // 向左两格：e(4) 拖到可见格 1 → [a e b c d]
        let (mut app, _) = setup(mk());
        app.begin_drag(app_item('e'), 0, 4);
        app.drop_at(0, 1, false);
        assert_eq!(ids(&app.layout), vec!['a', 'e', 'b', 'c', 'd'], "向左移动两格");

        // 放回原位（未压图标）：no-op
        let (mut app, _) = setup(mk());
        app.begin_drag(app_item('c'), 0, 2);
        app.drop_at(0, 2, false);
        assert_eq!(ids(&app.layout), vec!['a', 'b', 'c', 'd', 'e'], "放回原位不变");

        // 紧邻右侧：a(0) 拖到可见格 0（b 滑进了 a 的位置）压在图标上 → 与 b 合并
        let (mut app, _) = setup(mk());
        app.begin_drag(app_item('a'), 0, 0);
        app.drop_at(0, 0, true);
        assert_eq!(app.layout.pages[0].items.len(), 4, "合并后少一项");
        assert!(matches!(app.layout.pages[0].items[0], Item::Folder { .. }), "首位是 a+b 合并的文件夹");

        // 紧邻右侧未压图标：no-op
        let (mut app, _) = setup(mk());
        app.begin_drag(app_item('a'), 0, 0);
        app.drop_at(0, 0, false);
        assert_eq!(ids(&app.layout), vec!['a', 'b', 'c', 'd', 'e'], "未压图标放回原位");
    }

    #[test]
    fn reorder_folder_moves_within() {
        let (mut app, _) = setup(vec![
            app_item('a'),
            app_item('b'),
            app_item('c'),
            Item::folder(Folder {
                id: "f".into(),
                name: "F".into(),
                apps: vec![
                    AppRef { bundle_id: "bid.x".into(), name: "X".into(), path: "/x".into() },
                    AppRef { bundle_id: "bid.y".into(), name: "Y".into(), path: "/y".into() },
                    AppRef { bundle_id: "bid.z".into(), name: "Z".into(), path: "/z".into() },
                ],
            }),
        ]);
        // 面板：7 列，宽 1400 → 列宽 200；指针在第 3 列（下标 2）
        let panel = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1400.0, 200.0));
        let pos = egui::pos2(2.0 * 200.0 + 100.0, 60.0);
        app.reorder_folder("f", 0, pos, panel);
        if let Item::Folder { folder } = &app.layout.pages[0].items[3] {
            let names: Vec<char> = folder.apps.iter().map(|x| x.name.chars().next().unwrap()).collect();
            assert_eq!(names, vec!['Y', 'Z', 'X'], "X 移到第 3 位");
        } else {
            panic!("应为文件夹");
        }
    }

    #[test]
    fn move_app_between_folders() {
        let (mut app, _) = setup(vec![
            Item::folder(Folder {
                id: "fa".into(),
                name: "A".into(),
                apps: vec![
                    AppRef { bundle_id: "bid.x".into(), name: "X".into(), path: "/x".into() },
                    AppRef { bundle_id: "bid.y".into(), name: "Y".into(), path: "/y".into() },
                ],
            }),
            Item::folder(Folder {
                id: "fb".into(),
                name: "B".into(),
                apps: vec![
                    AppRef { bundle_id: "bid.z".into(), name: "Z".into(), path: "/z".into() },
                ],
            }),
        ]);
        app.move_app_between_folders("fa", 0, "fb");
        // A 只剩 1 个应用 → 解散
        assert!(matches!(app.layout.pages[0].items[0], Item::App { .. }), "A 解散为应用");
        if let Item::Folder { folder } = &app.layout.pages[0].items[1] {
            assert_eq!(folder.apps.len(), 2, "X 移入 B");
        } else {
            panic!("B 应为文件夹");
        }
    }

    #[test]
    fn drop_on_icon_merges_only_when_on_icon() {
        let mk = || vec![app_item('a'), app_item('b'), app_item('c')];
        // 压在图标上：合并（拖 a 到可见格 1 = c 的位置 → 与 c 合并）
        let (mut app, _) = setup(mk());
        app.begin_drag(app_item('a'), 0, 0);
        app.drop_at(0, 1, true);
        assert_eq!(app.layout.pages[0].items.len(), 2, "合并后少一项");
        assert!(
            matches!(app.layout.pages[0].items[1], Item::Folder { .. }),
            "第二项是合并出的文件夹（可见格 1 是 c）"
        );

        // 多选拖到文件夹图标上：全部加入
        let (mut app, _) = setup(vec![
            app_item('a'),
            app_item('b'),
            Item::folder(Folder {
                id: "f".into(),
                name: "F".into(),
                apps: vec![
                    AppRef { bundle_id: "bid.x".into(), name: "X".into(), path: "/x".into() },
                    AppRef { bundle_id: "bid.y".into(), name: "Y".into(), path: "/y".into() },
                ],
            }),
        ]);
        app.selected.insert("bid.a".into());
        app.selected.insert("bid.b".into());
        app.begin_drag(app_item('a'), 0, 0);
        // a 被抽走后文件夹可见格为 1
        app.drop_at(0, 1, true);
        assert_eq!(app.layout.pages[0].items.len(), 1, "只剩文件夹");
        if let Item::Folder { folder } = &app.layout.pages[0].items[0] {
            assert_eq!(folder.apps.len(), 4, "a、b 加入文件夹");
        } else {
            panic!("应为文件夹");
        }

        // 没压在图标上（格子空隙）：普通移动
        let (mut app, _) = setup(mk());
        app.begin_drag(app_item('a'), 0, 0);
        app.drop_at(0, 1, false);
        assert_eq!(app.layout.pages[0].items.len(), 3, "不合并");
        assert!(matches!(app.layout.pages[0].items[0], Item::App { .. }), "仍是 App");
        assert_eq!(
            app.layout.pages[0].items.iter().map(|i| i.bundle_id().chars().last().unwrap()).collect::<Vec<_>>(),
            vec!['b', 'a', 'c'],
            "a 移到 b 之后"
        );
    }
}
