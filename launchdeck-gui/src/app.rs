//! LaunchDeck 主应用状态

use crate::icons::IconCache;
use crate::native;
use crate::layout::*;
use crate::layout_store;
use crate::ui;
use eframe::egui;
use launchdeck_core::{AppInfo, AppSettings, DisplayMode, Theme};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Tab { Launchpad, Uninstall, Settings }

/// 拖拽状态
pub struct DragState {
    pub item: Item,
    pub from_page: usize,
    pub from_index: usize,
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
}

impl LaunchDeckApp {
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
            let list = launchdeck_core::AppScanner::default().scan().unwrap_or_default();
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
                self.apps = list;
                self.icons.set_manifest(manifest);
                self.resolve_paths();
                layout_store::build_from_apps(&self.apps, &mut self.layout);
                layout_store::apply_sort(&mut self.layout);
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

    fn clamp_page(&mut self) {
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
        crate::native::activate_app();
        crate::native::set_window_level(if self.settings.display_mode == DisplayMode::Fullscreen {
            25
        } else {
            21
        });
        // 激活之后再设一次展示选项（仅设置一次会被系统在激活时重置）
        crate::native::set_menu_bar_hidden(false);
        self.reload_wallpaper_if_changed();
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

    /// 把当前选中的应用一起拖到目标位置；拖到某个 App 上时合并为文件夹
    pub fn drop_selected(&mut self, to_page: usize, to_index: usize) {
        if self.selected.is_empty() {
            return;
        }
        let target = self
            .layout
            .pages
            .get(to_page)
            .and_then(|p| p.items.get(to_index))
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

        // 拖到普通 App 上：合并成新文件夹
        if let Some(Item::App { app: ta }) = target {
            let mut apps = picked.clone();
            if !ids.contains(&ta.bundle_id) {
                apps.push(ta.clone());
            }
            let mut idx = to_index;
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
            let mut idx = to_index;
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
        let mut lines = String::new();
        for a in &refs {
            lines.push_str(&format!(
                "delete POSIX file \"{}\"\n",
                a.path.replace('"', "\\\"")
            ));
        }
        let script = format!("tell application \"Finder\"\n{lines}end tell");
        let _ = Command::new("/usr/bin/osascript").args(["-e", &script]).output();
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

    /// 把扫描到但布局里没有的新应用追加到末尾
    pub fn sync_new_apps(&mut self) {
        self.layout.imported = false;
        let _ = crate::layout_store::save(&self.layout);
        self.start_scan();
        self.status = "正在同步新应用…".into();
    }

    /// 用原版数据库恢复布局
    pub fn import_original_layout(&mut self) {
        if let Some(imp) = crate::orig_layout::import() {
            self.layout.pages = imp.pages;
            self.layout.hidden = imp.hidden;
            self.layout.added_at = imp.added_at;
            self.layout.imported = true;
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
        let script = format!(
            "tell application \"Finder\" to delete POSIX file \"{}\"",
            app.path.replace('"', "\\\"")
        );
        let _ = Command::new("/usr/bin/osascript").args(["-e", &script]).output();
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
        });
    }

    /// 放置到目标位置：
    /// - 目标是文件夹 → 加入该文件夹
    /// - 目标是 App 且来源也是 App → 合并成新文件夹
    /// - 否则普通移动
    pub fn drop_at(&mut self, to_page: usize, to_index: usize) {
        let Some(drag) = self.drag.take() else { return };
        if self.multi_drag {
            self.multi_drag = false;
            self.drop_selected(to_page, to_index);
            return;
        }
        let src_id = drag.item.bundle_id().to_string();
        let target = self
            .layout
            .pages
            .get(to_page)
            .and_then(|p| p.items.get(to_index))
            .cloned();

        if drag.from_page == to_page && drag.from_index == to_index {
            return;
        }

        match target {
            // 拖进文件夹
            Some(Item::Folder { folder }) if folder.id != src_id => {
                self.remove_item_at(drag.from_page, drag.from_index);
                if let Some(f) = self.folder_mut(&folder.id) {
                    for a in drag.item.apps() {
                        if !f.apps.iter().any(|x| x.bundle_id == a.bundle_id) {
                            f.apps.push(a);
                        }
                    }
                }
            }
            // 两个 App 合并成文件夹
            Some(Item::App { app: ta })
                if matches!(drag.item, Item::App { .. }) && ta.bundle_id != src_id =>
            {
                let mut apps = drag.item.apps();
                apps.extend(ta_apps(&self.layout, to_page, to_index));
                // 同一页内向前拖动时，移除来源后目标下标会左移
                let target_idx = if drag.from_page == to_page && drag.from_index < to_index {
                    to_index - 1
                } else {
                    to_index
                };
                self.remove_item_at(drag.from_page, drag.from_index);
                self.remove_item_at(to_page, target_idx);
                let mut idx = to_index;
                if drag.from_page == to_page && drag.from_index < to_index {
                    idx = idx.saturating_sub(1);
                }
                let id = format!("folder-{}", layout_store::now_ts());
                let folder = Folder {
                    id: id.clone(),
                    name: "未命名".into(),
                    apps,
                };
                self.insert_item(to_page, idx, Item::folder(folder));
                self.editing_folder = Some(id.clone());
                self.edit_text = "未命名".into();
                self.focus_editing = true;
                self.open_folder = Some(id);
            }
            // 普通移动
            _ => {
                self.remove_item_at(drag.from_page, drag.from_index);
                let mut idx = to_index;
                if drag.from_page == to_page && drag.from_index < to_index {
                    idx = idx.saturating_sub(1);
                }
                self.insert_item(to_page, idx, drag.item);
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
            Self::apply_display_mode(ctx, mode);
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

    pub fn apply_display_mode(ctx: &egui::Context, mode: DisplayMode) {
        crate::native::set_menu_bar_hidden(false);
        crate::native::set_window_level(if mode == DisplayMode::Fullscreen { 25 } else { 21 });
        let screen = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(2048.0, 1152.0));
        if mode == DisplayMode::Fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(screen));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(0.0, 0.0)));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            // 原版窗口模式是无边框的整块毛玻璃，不要系统标题栏
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
            let size = egui::vec2(1200.0, 800.0);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                (screen.x - size.x) / 2.0,
                (screen.y - size.y - 28.0) / 2.0,
            )));
        }
    }

    pub fn reorganize(&mut self) {
        layout_store::apply_sort(&mut self.layout);
        self.clamp_page();
        let _ = layout_store::save(&self.layout);
        self.status = "已重新整理布局".into();
    }
}

impl eframe::App for LaunchDeckApp {
    /// 透明窗口：设置/卸载页只画各自的浮层，四周透出真实桌面
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
            Self::apply_display_mode(ctx, self.settings.display_mode);
            self.reposition = 8; // 恢复全屏后重新贴到 (0,0)，否则可能被菜单栏挤到 y=30
            self.ignore_focus_frames = 6;
        }
        self.prev_tab = self.tab;
        self.prev_section = self.settings_section;


        if self.first_frame {
            self.first_frame = false;
            // 直接以设置页启动时不要再套全屏尺寸，否则会覆盖上面的 400×610
            if self.tab != Tab::Settings {
                Self::apply_display_mode(ctx, self.settings.display_mode);
            }
            self.reposition = 6;
        }

        // 无边框窗口在首帧可能被系统压在菜单栏下方，多校正几帧
        if self.reposition > 0 && self.tab != Tab::Settings {
            self.reposition -= 1;
            if self.settings.display_mode == DisplayMode::Fullscreen {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(0.0, 0.0)));
                if let Some(size) = ctx.input(|i| i.viewport().monitor_size) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                }
                crate::native::set_menu_bar_hidden(false);
            }
        }

        if !self.native_ready {
            self.native_ready = true;
            native::register_f4();
            native::install_swipe_tap();
            native::install_reopen_handler();
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
        self.poll_wallpaper(ctx);
        if self.scanning {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
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
