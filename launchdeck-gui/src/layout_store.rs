//! 布局持久化 + 从扫描结果自动组建布局

use crate::layout::*;
use std::collections::HashSet;
use std::path::PathBuf;

fn layout_path() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("LaunchDeck");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("layout.json")
}

pub fn load() -> Option<LaunchLayout> {
    if let Ok(data) = std::fs::read_to_string(layout_path()) {
        if let Ok(mut layout) = serde_json::from_str::<LaunchLayout>(&data) {
            normalize(&mut layout);
            return Some(layout);
        }
    }
    // 首次运行：优先导入原版 LaunchOS 的布局（分页 / 文件夹 / 顺序）
    if let Some(imported) = crate::orig_layout::import() {
        let mut layout = LaunchLayout::default();
        layout.pages = imported.pages;
        layout.hidden = imported.hidden;
        layout.added_at = imported.added_at;
        layout.imported = true;
        normalize(&mut layout);
        let _ = save(&layout);
        return Some(layout);
    }
    None
}

pub fn save(layout: &LaunchLayout) -> std::io::Result<()> {
    let data = serde_json::to_string_pretty(layout)?;
    std::fs::write(layout_path(), data)
}

/// 从扫描结果自动组建布局（首次运行或新增应用时）
pub fn build_from_apps(apps: &[launchdeck_core::AppInfo], existing: &mut LaunchLayout) {
    let known: HashSet<String> = existing.all_bundle_ids();
    let mut new_apps: Vec<AppRef> = Vec::new();

    for a in apps {
        let bid = if a.bundle_id.is_empty() {
            a.path.to_string_lossy().to_string()
        } else {
            a.bundle_id.clone()
        };
        if known.contains(&bid) { continue; }
        // 隐藏的应用不加入
        if existing.hidden.contains(&bid) { continue; }
        new_apps.push(AppRef {
            bundle_id: bid.clone(),
            name: a.name.clone(),
            path: a.path.to_string_lossy().to_string(),
        });
        existing.added_at.entry(bid).or_insert_with(now_ts);
    }

    // 从原版导入的布局不自动追加新应用，保持与原版一致
    if existing.imported {
        new_apps.clear();
    }

    if new_apps.is_empty() {
        // 仍需清理已卸载的应用
        let alive: HashSet<String> = apps
            .iter()
            .map(|a| {
                if a.bundle_id.is_empty() {
                    a.path.to_string_lossy().to_string()
                } else {
                    a.bundle_id.clone()
                }
            })
            .collect();
        for bid in existing.all_bundle_ids() {
            if alive.contains(&bid) {
                continue;
            }
            if !item_path_exists(existing, &bid) {
                existing.remove_app(&bid);
            }
        }
        normalize(existing);
        return;
    }

    // 追加到末尾，按容量自动开新页
    let cap = existing.capacity();
    for app in new_apps {
        loop {
            let last = existing.pages.len() - 1;
            if existing.pages[last].items.len() < cap {
                existing.pages[last].items.push(Item::app(app));
                break;
            } else {
                existing.pages.push(Page::default());
            }
        }
    }

    // 移除已不存在的应用（卸载后）
    let alive: HashSet<String> = apps
        .iter()
        .map(|a| if a.bundle_id.is_empty() {
            a.path.to_string_lossy().to_string()
        } else {
            a.bundle_id.clone()
        })
        .collect();
    let present = existing.all_bundle_ids();
    for bid in present {
        if !alive.contains(&bid) && !item_path_exists(existing, &bid) {
            existing.remove_app(&bid);
        }
    }

    normalize(existing);
}

/// 按当前行列容量重新分页（保持现有顺序）
pub fn reflow(layout: &mut LaunchLayout) {
    let items: Vec<Item> = layout.pages.drain(..).flat_map(|p| p.items).collect();
    let cap = layout.capacity().max(1);
    let mut pages = vec![Page::default()];
    for item in items {
        if pages.last().map(|p| p.items.len()).unwrap_or(0) >= cap {
            pages.push(Page::default());
        }
        pages.last_mut().unwrap().items.push(item);
    }
    layout.pages = pages;
    normalize(layout);
}

/// 去掉空页、保证至少一页
pub fn normalize(layout: &mut LaunchLayout) {
    layout.pages.retain(|p| !p.items.is_empty());
    if layout.pages.is_empty() {
        layout.pages.push(Page::default());
    }
}

pub fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 按排序模式重排（仅 App 项，文件夹保持原位）
pub fn apply_sort(layout: &mut LaunchLayout) {
    if layout.sort_mode == SortMode::Custom { return; }

    // 收集所有 item 到扁平列表
    let mut items: Vec<Item> = layout.pages.drain(..).flat_map(|p| p.items).collect();

    let mode = layout.sort_mode;
    let asc = layout.ascending;
    let last_used = layout.last_used.clone();
    let added_at = layout.added_at.clone();

    items.sort_by(|a, b| {
        let an = a.display_name().to_lowercase();
        let bn = b.display_name().to_lowercase();
        let ord = match mode {
            SortMode::Name => an.cmp(&bn),
            SortMode::DateAdded => {
                let ta = a.apps().first().and_then(|x| added_at.get(&x.bundle_id)).copied().unwrap_or(0);
                let tb = b.apps().first().and_then(|x| added_at.get(&x.bundle_id)).copied().unwrap_or(0);
                ta.cmp(&tb)
            }
            SortMode::LastUsed => {
                let ta = a.apps().first().and_then(|x| last_used.get(&x.bundle_id)).copied().unwrap_or(0);
                let tb = b.apps().first().and_then(|x| last_used.get(&x.bundle_id)).copied().unwrap_or(0);
                ta.cmp(&tb)
            }
            SortMode::Custom => std::cmp::Ordering::Equal,
        };
        if asc { ord } else { ord.reverse() }
    });

    // 重新分页
    let cap = layout.capacity();
    let mut pages = vec![Page::default()];
    for item in items {
        let last = pages.len() - 1;
        if pages[last].items.len() >= cap {
            pages.push(Page::default());
        }
        let last = pages.len() - 1;
        pages[last].items.push(item);
    }
    layout.pages = pages;
    normalize(layout);
}

/// 布局中该 bundle id 对应的 App 路径是否仍存在
fn item_path_exists(layout: &LaunchLayout, bundle_id: &str) -> bool {
    for page in &layout.pages {
        for item in &page.items {
            for app in item.apps() {
                if app.bundle_id == bundle_id && std::path::Path::new(&app.path).exists() {
                    return true;
                }
            }
        }
    }
    false
}
