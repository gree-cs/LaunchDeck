use crate::{error::*, model::*};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;
use dirs;

/// 应用扫描器（纯文件系统实现，无平台依赖）
pub struct AppScanner {
    /// 扫描根目录
    roots: Vec<PathBuf>,
    /// 忽略扩展名
    ignore_exts: Vec<&'static str>,
}

impl Default for AppScanner {
    fn default() -> Self {
        let mut roots = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
            PathBuf::from("/System/Applications/Utilities"),
            PathBuf::from("/Applications/Utilities"),
            dirs::home_dir().unwrap_or_default().join("Applications"),
        ];
        // 用户在设置里添加的自定义扫描目录
        for src in crate::config::get_settings().custom_sources {
            let p = PathBuf::from(src);
            if p.is_dir() && !roots.contains(&p) {
                roots.push(p);
            }
        }
        roots.retain(|p| p.exists());

        Self {
            roots,
            ignore_exts: vec!["dSYM", "framework", "plugin", "kext", "xpc"],
        }
    }
}

impl AppScanner {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self {
            roots,
            ignore_exts: vec!["dSYM", "framework", "plugin", "kext", "xpc"],
        }
    }

    fn is_ignored(&self, path: &Path) -> bool {
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => self.ignore_exts.contains(&ext),
            None => false,
        }
    }

    /// 扫描所有应用
    pub fn scan(&self) -> Result<Vec<AppInfo>> {
        let mut by_id: HashMap<String, AppInfo> = HashMap::new();

        for root in &self.roots {
            for entry in WalkDir::new(root)
                .max_depth(3)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("app") {
                    continue;
                }
                if self.is_ignored(path) {
                    continue;
                }
                // .app 目录直接解析；非目录的 .app（替身文件 / 符号链接）
                // 也收集，由前端解析替身指向的目标后决定是否保留
                if !entry.file_type().is_dir() {
                    if let Ok(mut app) = Self::path_to_app_info(path) {
                        app.is_alias = true;
                        let key = app.id.clone();
                        by_id.entry(key).or_insert(app);
                    }
                    continue;
                }
                if let Ok(app) = Self::path_to_app_info(path) {
                    // 用 bundle_id 优先去重，无 bundle_id 则用路径
                    let key = if app.bundle_id.is_empty() {
                        app.id.clone()
                    } else {
                        app.bundle_id.clone()
                    };
                    by_id.entry(key).or_insert(app);
                }
            }
        }

        let mut apps: Vec<AppInfo> = by_id.into_values().collect();
        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(apps)
    }

    fn path_to_app_info(path: &Path) -> Result<AppInfo> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let info_plist = path.join("Contents/Info.plist");
        let bundle_id = Self::get_bundle_id(&info_plist).unwrap_or_default();
        let version = Self::get_version(&info_plist).unwrap_or_else(|| "1.0".to_string());
        let category = Self::get_category(&info_plist);

        let metadata = std::fs::metadata(path)?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let size = Self::calculate_dir_size(path);

        let is_system = path.starts_with("/System/");

        Ok(AppInfo {
            id: path.to_string_lossy().to_string(),
            name,
            path: path.to_path_buf(),
            version,
            bundle_id,
            icon_path: None,
            category,
            size,
            modified,
            is_hidden: false,
            is_system,
            is_alias: false,
            alias_path: None,
        })
    }

    /// 读取 Info.plist（支持 XML 与二进制格式，后者在系统应用 / Office 中很常见）
    fn plist_value(plist: &Path, key: &str) -> Option<String> {
        if let Ok(value) = plist::Value::from_file(plist) {
            if let Some(dict) = value.into_dictionary() {
                if let Some(v) = dict.get(key) {
                    if let Some(s) = v.as_string() {
                        return Some(s.to_string());
                    }
                }
            }
        }
        // XML 文本回退
        let content = std::fs::read_to_string(plist).ok()?;
        let pos = content.find(&format!("<key>{}</key>", key))?;
        let after = &content[pos..];
        let open = after.find("<string>")?;
        let start = open + "<string>".len();
        let end = after[start..].find("</string>")?;
        Some(after[start..start + end].trim().to_string())
    }

    fn get_bundle_id(plist: &Path) -> Option<String> {
        Self::plist_value(plist, "CFBundleIdentifier")
    }

    fn get_version(plist: &Path) -> Option<String> {
        Self::plist_value(plist, "CFBundleShortVersionString")
            .or_else(|| Self::plist_value(plist, "CFBundleVersion"))
    }

    fn get_category(plist: &Path) -> Option<String> {
        Self::plist_value(plist, "LSApplicationCategoryType")
    }

    /// 计算目录大小
    pub fn calculate_dir_size(path: &Path) -> u64 {
        WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum()
    }

    /// 读取指定 .app 的 CFBundleIdentifier（前端解析替身后补全信息用）
    pub fn read_bundle_id(path: &Path) -> Option<String> {
        Self::get_bundle_id(&path.join("Contents/Info.plist"))
    }
}

/// 异步扫描（后台线程）
pub async fn scan_apps_async() -> Result<Vec<AppInfo>> {
    tokio::task::spawn_blocking(|| AppScanner::default().scan())
        .await
        .map_err(|e| LaunchDeckError::Scan(e.to_string()))?
}
