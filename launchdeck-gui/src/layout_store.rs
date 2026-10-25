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

/// 从后面的页面把应用往前搬到 `page`，直到该页填满（整理当前页的空位）
pub fn fill_page_from_later(layout: &mut LaunchLayout, page: usize) {
    let cap = layout.capacity().max(1);
    if page >= layout.pages.len() {
        return;
    }
    let mut src = page + 1;
    while src < layout.pages.len() && layout.pages[page].items.len() < cap {
        if layout.pages[src].items.is_empty() {
            src += 1;
            continue;
        }
        let item = layout.pages[src].items.remove(0);
        layout.pages[page].items.push(item);
    }
    layout.pages.retain(|p| !p.items.is_empty());
    if layout.pages.is_empty() {
        layout.pages.push(Page::default());
    }
}

/// 把所选页面里的全部应用收拢到这些页（保持相对顺序，可能减少页数），其他页不动
pub fn compact_pages(layout: &mut LaunchLayout, selected: &[usize]) {
    if selected.is_empty() {
        return;
    }
    let mut sel: Vec<usize> = selected.to_vec();
    sel.sort_unstable();
    sel.dedup();
    let cap = layout.capacity().max(1);
    let first = sel[0];

    let mut picked: Vec<Vec<Item>> = sel
        .iter()
        .map(|&pi| layout.pages.get(pi).map(|p| p.items.clone()).unwrap_or_default())
        .collect();
    let flat: Vec<Item> = picked.drain(..).flatten().collect();

    let mut kept: Vec<Page> = Vec::new();
    for (i, p) in layout.pages.drain(..).enumerate() {
        if !sel.contains(&i) {
            kept.push(p);
        }
    }

    let at = first.min(kept.len());
    let mut chunks: Vec<Page> = Vec::new();
    for chunk in flat.chunks(cap) {
        let mut pg = Page::default();
        pg.items = chunk.to_vec();
        chunks.push(pg);
    }
    if chunks.is_empty() {
        chunks.push(Page::default());
    }
    let mut tail = kept.split_off(at);
    kept.extend(chunks);
    kept.append(&mut tail);
    layout.pages = kept;
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

/// 按排序模式重排。文件夹作为整体参与排序；
/// `sort_folders_too` 开启时文件夹内的应用也按同样规则排序。
pub fn apply_sort(layout: &mut LaunchLayout, settings: &launchdeck_core::AppSettings) {
    if layout.sort_mode == SortMode::Custom { return; }

    // 收集所有 item 到扁平列表
    let mut items: Vec<Item> = layout.pages.drain(..).flat_map(|p| p.items).collect();

    let mode = layout.sort_mode;
    let asc = layout.ascending;
    let last_used = layout.last_used.clone();
    let added_at = layout.added_at.clone();

    // 文件夹内的应用也排序
    if settings.sort_folders_too {
        for it in &mut items {
            if let Item::Folder { folder } = it {
                folder.apps.sort_by(|a, b| {
                    let ord = app_order(a, b, mode, &added_at, &last_used);
                    if asc { ord } else { ord.reverse() }
                });
            }
        }
    }

    // 排序时 Apple 应用相对用户应用的位置（文件夹按用户应用处理）
    let group = |it: &Item| -> u8 {
        let is_system = it.apps().first().map(|a| a.path.starts_with("/System/")).unwrap_or(false);
        match (settings.apple_placement, is_system) {
            (launchdeck_core::ApplePlacement::Before, true) => 0,
            (launchdeck_core::ApplePlacement::After, true) => 2,
            _ => 1,
        }
    };

    items.sort_by(|a, b| {
        let (ga, gb) = (group(a), group(b));
        if ga != gb {
            return ga.cmp(&gb);
        }
        let ord = app_order(
            a.apps().first().unwrap_or(&DUMMY_APP),
            b.apps().first().unwrap_or(&DUMMY_APP),
            mode,
            &added_at,
            &last_used,
        );
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

static DUMMY_APP: crate::layout::AppRef = crate::layout::AppRef {
    bundle_id: std::string::String::new(),
    name: std::string::String::new(),
    path: std::string::String::new(),
};

fn app_order(
    a: &crate::layout::AppRef,
    b: &crate::layout::AppRef,
    mode: SortMode,
    added_at: &std::collections::HashMap<String, u64>,
    last_used: &std::collections::HashMap<String, u64>,
) -> std::cmp::Ordering {
    let an = a.name.to_lowercase();
    let bn = b.name.to_lowercase();
    let ord = match mode {
        SortMode::Name => an.cmp(&bn),
        SortMode::DateAdded => {
            let ta = added_at.get(&a.bundle_id).copied().unwrap_or(0);
            let tb = added_at.get(&b.bundle_id).copied().unwrap_or(0);
            ta.cmp(&tb)
        }
        SortMode::LastUsed => {
            let ta = last_used.get(&a.bundle_id).copied().unwrap_or(0);
            let tb = last_used.get(&b.bundle_id).copied().unwrap_or(0);
            ta.cmp(&tb)
        }
        SortMode::Custom => std::cmp::Ordering::Equal,
    };
    ord.then_with(|| an.cmp(&bn))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::*;
    use launchdeck_core::{ApplePlacement, AppSettings};

    fn app(id: &str, name: &str, path: &str) -> Item {
        Item::app(AppRef {
            bundle_id: id.into(),
            name: name.into(),
            path: path.into(),
        })
    }

    fn settings() -> AppSettings {
        AppSettings {
            sort_folders_too: true,
            apple_placement: ApplePlacement::Mixed,
            ..Default::default()
        }
    }

    #[test]
    fn fill_page_from_later_moves_items_forward() {
        let mut l = LaunchLayout {
            columns: 4,
            rows: 1,
            ..Default::default()
        };
        l.pages = vec![
            Page { items: vec![app("a", "A", "/a"), app("b", "B", "/b")] },
            Page { items: vec![app("c", "C", "/c"), app("d", "D", "/d"), app("e", "E", "/e")] },
        ];
        fill_page_from_later(&mut l, 0);
        assert_eq!(l.pages[0].items.len(), 4, "第 0 页应被填满 4 格");
        assert_eq!(l.pages[0].items[2].bundle_id(), "c");
        assert_eq!(l.pages[1].items.len(), 1);
        assert_eq!(l.pages[1].items[0].bundle_id(), "e");
    }

    #[test]
    fn compact_pages_squashes_selected() {
        let mut l = LaunchLayout {
            columns: 2,
            rows: 1,
            ..Default::default()
        };
        l.pages = vec![
            Page { items: vec![app("a", "A", "/a"), app("b", "B", "/b")] },
            Page { items: vec![app("c", "C", "/c")] },
            Page { items: vec![app("d", "D", "/d"), app("e", "E", "/e")] },
        ];
        // 只选第 1、2 页 → 收拢为按容量分页的两页（c,d | e），第 0 页不动
        compact_pages(&mut l, &[1, 2]);
        assert_eq!(l.pages.len(), 3);
        assert_eq!(l.pages[0].items[0].bundle_id(), "a");
        assert_eq!(l.pages[1].items[0].bundle_id(), "c");
        assert_eq!(l.pages[1].items[1].bundle_id(), "d");
        assert_eq!(l.pages[2].items[0].bundle_id(), "e");
    }

    #[test]
    fn apply_sort_sorts_folder_contents_too() {
        let mut l = LaunchLayout {
            columns: 7,
            rows: 5,
            sort_mode: SortMode::Name,
            ..Default::default()
        };
        let folder = Item::folder(Folder {
            id: "f1".into(),
            name: "F".into(),
            apps: vec![
                AppRef { bundle_id: "z".into(), name: "Zeta".into(), path: "/z".into() },
                AppRef { bundle_id: "m".into(), name: "Middle".into(), path: "/m".into() },
                AppRef { bundle_id: "a".into(), name: "Alpha".into(), path: "/a".into() },
            ],
        });
        l.pages[0].items = vec![folder, app("x", "Xray", "/x")];
        apply_sort(&mut l, &settings());
        let Item::Folder { folder } = &l.pages[0].items[0] else {
            panic!("文件夹应参与排序");
        };
        let names: Vec<&str> = folder.apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Middle", "Zeta"], "文件夹内应用也应排序");
    }

    #[test]
    fn apply_sort_apple_placement_groups_system_first() {
        let mut l = LaunchLayout {
            columns: 7,
            rows: 5,
            sort_mode: SortMode::Name,
            ..Default::default()
        };
        l.pages[0].items = vec![
            app("u1", "Zulu", "/Applications/Zulu.app"),
            app("s1", "Notes", "/System/Applications/Notes.app"),
            app("u2", "Beta", "/Applications/Beta.app"),
        ];
        let mut s = settings();
        s.apple_placement = ApplePlacement::Before;
        apply_sort(&mut l, &s);
        let ids: Vec<&str> = l.pages[0].items.iter().map(|i| i.bundle_id()).collect();
        assert_eq!(ids[0], "s1", "系统应用应排最前");
        assert_eq!(ids[1], "u2");
        assert_eq!(ids[2], "u1");

        s.apple_placement = ApplePlacement::After;
        apply_sort(&mut l, &s);
        let ids: Vec<&str> = l.pages[0].items.iter().map(|i| i.bundle_id()).collect();
        assert_eq!(ids[2], "s1", "系统应用应排最后");
    }

    #[test]
    fn build_from_apps_appends_new_and_removes_dead() {
        let mut l = LaunchLayout {
            columns: 7,
            rows: 5,
            ..Default::default()
        };
        l.pages[0].items = vec![app("old", "Old", "/nonexistent/Old.app")];
        let apps = vec![
            launchdeck_core::AppInfo {
                id: "new".into(),
                name: "New".into(),
                path: "/Applications/New.app".into(),
                version: "1".into(),
                bundle_id: "new".into(),
                icon_path: None,
                category: None,
                size: 10,
                modified: std::time::SystemTime::UNIX_EPOCH,
                is_hidden: false,
                is_system: false,
                is_alias: false,
                alias_path: None,
            },
        ];
        build_from_apps(&apps, &mut l);
        let ids: Vec<&str> = l.pages[0].items.iter().map(|i| i.bundle_id()).collect();
        assert_eq!(ids, vec!["new"], "新应用应追加、失效应用应移除");
    }
}
