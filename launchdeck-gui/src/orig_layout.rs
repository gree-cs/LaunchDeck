//! 从原版 LaunchOS 的 Core Data 数据库导入分页 / 文件夹 / 顺序。
//!
//! 数据库：`~/Library/Application Support/LaunchOS/LaunchOS.sqlite`
//! 表：ZGROUPENTITY（每个顶层格子：App 或文件夹）、ZAPPENTITY（格子内的应用）。
//! 只读访问，使用系统自带的 sqlite3 输出 JSON。

use crate::layout::*;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

#[derive(Deserialize)]
struct RawGroup {
    #[serde(rename = "Z_PK")]
    pk: i64,
    #[serde(rename = "ZISFOLDER")]
    is_folder: Option<i64>,
    #[serde(rename = "ZPAGE")]
    page: Option<i64>,
    #[serde(rename = "ZNAME")]
    name: Option<String>,
}

#[derive(Deserialize)]
struct RawApp {
    #[serde(rename = "ZNAME")]
    name: Option<String>,
    #[serde(rename = "ZBUNDLEID")]
    bundle: Option<String>,
    #[serde(rename = "ZURL")]
    url: Option<String>,
    #[serde(rename = "ZGROUP")]
    group: Option<i64>,
    #[serde(rename = "ZHIDDEN")]
    hidden: Option<i64>,
}

pub struct Imported {
    pub pages: Vec<Page>,
    pub hidden: HashSet<String>,
    pub added_at: HashMap<String, u64>,
}

pub fn db_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("LaunchOS").join("LaunchOS.sqlite"))
}

pub fn import() -> Option<Imported> {
    let db = db_path()?;
    if !db.exists() {
        return None;
    }

    let groups: Vec<RawGroup> = query(
        &db,
        "SELECT Z_PK,ZISFOLDER,ZORDER,ZPAGE,ZNAME FROM ZGROUPENTITY ORDER BY ZPAGE,ZORDER;",
    )?;
    let apps: Vec<RawApp> = query(
        &db,
        "SELECT ZNAME,ZBUNDLEID,ZURL,ZGROUP,ZORDER,ZHIDDEN FROM ZAPPENTITY ORDER BY ZORDER;",
    )?;

    let mut by_group: HashMap<i64, Vec<RawApp>> = HashMap::new();
    for a in apps {
        if let Some(g) = a.group {
            by_group.entry(g).or_default().push(a);
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut pages: BTreeMap<i64, Vec<Item>> = BTreeMap::new();
    let mut hidden: HashSet<String> = HashSet::new();
    let mut added_at: HashMap<String, u64> = HashMap::new();

    let dbg = std::env::var("LAUNCHDECK_DEBUG_IMPORT").is_ok();
    for g in groups {
        let page = g.page.unwrap_or(0).max(0) as usize;
        let list = by_group.remove(&g.pk).unwrap_or_default();
        if dbg {
            eprintln!("[imp] group {} folder={:?} page={} apps={}", g.pk, g.is_folder, page, list.len());
        }

        if g.is_folder == Some(1) {
            let mut members: Vec<AppRef> = Vec::new();
            for a in &list {
                if a.hidden == Some(1) {
                    continue;
                }
                if let Some(app) = to_ref(a) {
                    added_at.entry(app.bundle_id.clone()).or_insert(now);
                    members.push(app);
                }
            }
            if members.is_empty() {
                continue;
            }
            pages
                .entry(page as i64)
                .or_default()
                .push(Item::folder(Folder {
                    id: format!("orig-{}", g.pk),
                    name: g.name.unwrap_or_else(|| "文件夹".into()),
                    apps: members,
                }));
        } else if let Some(a) = list.first() {
            if a.hidden == Some(1) {
                if let Some(app) = to_ref(a) {
                    hidden.insert(app.bundle_id);
                }
                continue;
            }
            if let Some(app) = to_ref(a) {
                added_at.entry(app.bundle_id.clone()).or_insert(now);
                pages
                    .entry(page as i64)
                    .or_default()
                    .push(Item::app(app));
            }
        }
    }

    let pages: Vec<Page> = pages
        .into_values()
        .map(|items| Page { items })
        .collect();
    if pages.is_empty() {
        return None;
    }

    Some(Imported {
        pages,
        hidden,
        added_at,
    })
}

fn to_ref(a: &RawApp) -> Option<AppRef> {
    let Some(url) = a.url.as_ref() else {
        eprintln!("[imp] skip no url: {:?}", a.name);
        return None;
    };
    let path = url_to_path(url)?;
    let name = a
        .name
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "应用".into());
    let bundle = a
        .bundle
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.clone());
    Some(AppRef {
        bundle_id: bundle,
        name,
        path,
    })
}

fn url_to_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.trim_end_matches('/');
    Some(percent_decode(rest))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query<T: for<'de> Deserialize<'de>>(db: &PathBuf, sql: &str) -> Option<Vec<T>> {
    let out = Command::new("/usr/bin/sqlite3")
        .args(["-json", "-readonly"])
        .arg(db)
        .arg(sql)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if text.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<Vec<T>>(&text).ok()
}
