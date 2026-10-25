//! E2E 测试钩子（仅 `--features e2e` 构建，生产构建不编译本模块的任何逻辑）。
//!
//! 用法：LAUNCHDECK_E2E=/tmp/e2e 启动后——
//! - 指令：写一行 `命令|参数` 到 /tmp/e2e/cmd，应用每帧读取并执行后删除；
//! - 状态：应用每帧把内部状态 JSON 写到 /tmp/e2e/state.json。

use crate::app::{LaunchDeckApp, Tab};
use std::sync::atomic::{AtomicU64, Ordering};

// e2e 伪造拖拽时的幽灵坐标（打包为 x<<32|y；0 = 无）
static GHOST_POS: AtomicU64 = AtomicU64::new(0);

/// 伪造拖拽的幽灵位置（真实运行时返回 None，走真实指针）
pub fn ghost_pos() -> Option<egui::Pos2> {
    let v = GHOST_POS.load(Ordering::SeqCst);
    if v == 0 {
        None
    } else {
        Some(egui::pos2((v >> 32) as f32, (v & 0xffff_ffff) as f32))
    }
}

#[cfg(feature = "e2e")]
pub fn tick(app: &mut LaunchDeckApp) {
    use std::io::Write as _;

    let Some(dir) = std::env::var("LAUNCHDECK_E2E").ok() else { return };
    let dir = std::path::PathBuf::from(dir);
    let cmd_path = dir.join("cmd");
    let state_path = dir.join("state.json");

    // 自动化点击不会转移键盘焦点，屏蔽「失焦自动收起」
    app.was_active = true;
    app.unfocused_frames = 0;

    // 读取并执行一条指令
    if let Ok(line) = std::fs::read_to_string(&cmd_path) {
        let _ = std::fs::remove_file(&cmd_path);
        let mut parts = line.trim().splitn(2, '|');
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("");
        match cmd {
            "noop" => {}
            "show" => app.show_launchpad(),
            "hide" => app.hide_window(),
            "set_page" => app.page = arg.parse().unwrap_or(0),
            "open_settings" => app.tab = Tab::Settings,
            "open_launchpad_tab" => app.tab = Tab::Launchpad,
            "set_settings_section" => app.settings_section = arg.parse().unwrap_or(0),
            "menu" => app.main_menu_open = arg == "1",
            "drag_clear" => {
                app.drag = None;
                app.open_folder = None;
                app.folder_closing = false;
            }
            "open_folder" => {
                app.open_folder = Some(arg.to_string());
                app.folder_anim = 1.0;
                app.folder_closing = false;
            }
            "drag_folder_out" => {
                // 伪造「从文件夹拖出第一个应用」：设置 from_folder 拖拽状态，
                // 用于截图验证文件夹实时少一个图标 + 幽灵跟随
                // 格式：drag_folder_out|fid|idx|x|y（x/y 为幽灵坐标，点单位）
                use crate::layout::{AppRef, Item};
                let mut parts = arg.splitn(4, '|');
                let fid = parts.next().unwrap_or("").to_string();
                let idx: usize = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                let gx: f32 = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
                let gy: f32 = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
                GHOST_POS.store(
                    ((gx as u64) << 32) | (gy as u64 & 0xffff_ffff),
                    Ordering::SeqCst,
                );
                let item = app
                    .layout
                    .pages
                    .iter()
                    .flat_map(|p| p.items.iter())
                    .find_map(|it| match it {
                        Item::Folder { folder } if folder.id == fid => {
                            folder.apps.get(idx).cloned().map(Item::app)
                        }
                        _ => None,
                    });
                if let Some(item) = item {
                    app.drag = Some(crate::app::DragState {
                        item,
                        from_page: usize::MAX,
                        from_index: idx,
                        from_folder: Some(fid),
                    });
                }
            }
            "select" => {
                if arg == "clear" {
                    app.selected.clear();
                } else {
                    app.selected.insert(arg.to_string());
                }
            }
            "filter" => app.set_category_filter(if arg == "none" { None } else { Some(arg.to_string()) }),
            "uninstall" => app.open_uninstall_detail(arg),
            "uninstall_confirm" => app.confirm_uninstall_detail(),
            "uninstall_quit" => app.quit_running_app(),
            "uninstall_cancel" => app.uninstall_detail = None,
            "tidy_open" => app.tidy_popup_open = arg == "1",
            "tidy_select" => {
                let i: usize = arg.parse().unwrap_or(0);
                if app.tidy_selected.contains(&i) {
                    app.tidy_selected.remove(&i);
                } else {
                    app.tidy_selected.insert(i);
                }
            }
            "tidy_run" => app.reorganize_selected_pages(),
            "rename" => {
                if let Some((bid, name)) = arg.split_once(' ') {
                    app.renaming_app = Some(bid.to_string());
                    app.edit_text = name.to_string();
                    app.commit_app_rename(name);
                    app.renaming_app = None;
                }
            }
            "set_browsing" => {
                app.layout.browsing_style = if arg == "scroll" {
                    crate::layout::BrowsingStyle::Scroll
                } else {
                    crate::layout::BrowsingStyle::Paged
                };
                let _ = crate::layout_store::save(&app.layout);
            }
            "reorganize" => app.reorganize(),
            "dock_add" => {
                if let Some(a) = find_app_ref(app, arg).cloned() {
                    app.add_to_dock(&a);
                }
            }
            "set_return_home" => {
                use launchdeck_core::ReturnHome;
                app.settings.return_home = match arg {
                    "always" => ReturnHome::Always,
                    "never" => ReturnHome::Never,
                    "5min" => ReturnHome::FiveMinutes,
                    "1min" => ReturnHome::OneMinute,
                    "30s" => ReturnHome::ThirtySeconds,
                    "15s" => ReturnHome::FifteenSeconds,
                    "5s" => ReturnHome::FiveSeconds,
                    _ => ReturnHome::Never,
                };
                let _ = launchdeck_core::save_settings(&app.settings);
            }
            "update_check" => app.spawn_update_check(),
            "quit" => std::process::exit(0),
            _ => {}
        }
    }

    // 导出状态
    let detail = app.uninstall_detail.as_ref().map(|d| {
        serde_json::json!({
            "name": d.name,
            "running": d.running,
            "kinds": d.residues.iter().map(|r| r.kind.label()).collect::<Vec<_>>(),
            "sizes": d.residues.iter().map(|r| r.size).collect::<Vec<_>>(),
            "checked": d.checked,
        })
    });
    let state = serde_json::json!({
        "visible": app.visible,
        "closing": app.closing,
        "tab": match app.tab {
            Tab::Launchpad => "Launchpad",
            Tab::Settings => "Settings",
            Tab::Uninstall => "Uninstall",
        },
        "settings_section": app.settings_section,
        "page": app.page,
        "pages": app.layout.page_count(),
        "items_total": app.layout.pages.iter().map(|p| p.items.len()).sum::<usize>(),
        "browsing": match app.layout.browsing_style {
            crate::layout::BrowsingStyle::Paged => "Paged",
            crate::layout::BrowsingStyle::Scroll => "Scroll",
        },
        "category_filter": app.category_filter,
        "filtered_items": app.filtered_items().len(),
        "main_menu_open": app.main_menu_open,
        "tidy_popup_open": app.tidy_popup_open,
        "tidy_selected": app.tidy_selected.iter().copied().collect::<Vec<_>>(),
        "renaming_app": app.renaming_app,
        "uninstall_detail": detail,
        "scanning": app.scanning,
        "apps_count": app.apps.len(),
        "update_status": app.update_status,
        "status": app.status,
        "return_home": app.settings.return_home.label(),
        "has_e2e_app": app.apps.iter().any(|a| a.bundle_id == "app.launchdeck.e2e"
            || a.id.ends_with("LaunchDeckE2E.app")),
        "folders": app.layout.pages.iter().flat_map(|p| p.items.iter()).filter_map(|it| match it {
            crate::layout::Item::Folder { folder } => Some(format!("{}({})", folder.id, folder.apps.len())),
            _ => None,
        }).collect::<Vec<_>>(),
        "open_folder": app.open_folder,
        "drag_from": app.drag.as_ref().map(|d| format!("{:?}", d.from_folder)),
    });
    // 原子写：先写临时文件再改名，避免读到半截 JSON
    let tmp = dir.join("state.json.tmp");
    if std::fs::write(&tmp, format!("{state}\n")).is_ok() {
        let _ = std::fs::rename(&tmp, &state_path);
    }
}

#[cfg(not(feature = "e2e"))]
pub fn tick(_app: &mut LaunchDeckApp) {}

#[cfg(feature = "e2e")]
pub use helper::find_app_ref;

#[cfg(feature = "e2e")]
mod helper {
    use crate::app::LaunchDeckApp;
    use crate::layout::Item;

    /// 按 bundle id（或路径）找到应用的 AppRef（含文件夹内）
    pub fn find_app_ref<'a>(app: &'a LaunchDeckApp, bid: &str) -> Option<&'a crate::layout::AppRef> {
        for page in &app.layout.pages {
            for it in &page.items {
                match it {
                    Item::App { app: a } => {
                        if a.bundle_id == bid || a.path == bid {
                            return Some(a);
                        }
                    }
                    Item::Folder { folder } => {
                        if let Some(a) = folder.apps.iter().find(|a| a.bundle_id == bid || a.path == bid) {
                            return Some(a);
                        }
                    }
                }
            }
        }
        None
    }
}
