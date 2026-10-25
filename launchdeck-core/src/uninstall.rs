//! 深度卸载：应用卸载后残留文件的扫描与分类。
//!
//! 纯文件系统实现（无平台依赖）。所有删除动作由前端执行（统一移入废纸篓），
//! 本模块只负责找出候选路径并统计大小。

use std::path::{Path, PathBuf};

/// 残留类别（顺序即设置页展示顺序）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidueKind {
    /// .app 本体（普通卸载只删这个）
    AppFiles,
    /// ~/Library/Application Support/…
    AppSupport,
    /// ~/Library/Containers/…（沙盒数据）
    Container,
    /// ~/Library/Group Containers/…
    GroupContainer,
    /// ~/Library/WebKit/… 及容器内 WebKit 数据
    WebKit,
    /// ~/Library/Preferences/…
    Preferences,
    /// ~/Library/Saved Application State/…
    SavedState,
    /// ~/Library/Caches/…
    Caches,
    /// LaunchAgents / LaunchDaemons 里引用该应用的启动项
    LaunchItems,
    /// ~/Library/Logs/DiagnosticReports/ 崩溃与诊断日志
    CrashLogs,
    /// 应用脚本与其他
    Other,
}

impl ResidueKind {
    pub fn label(&self) -> &'static str {
        match self {
            ResidueKind::AppFiles => "应用文件",
            ResidueKind::AppSupport => "支持文件",
            ResidueKind::Container => "容器",
            ResidueKind::GroupContainer => "容器组",
            ResidueKind::WebKit => "WebKit 数据",
            ResidueKind::Preferences => "偏好设置",
            ResidueKind::SavedState => "已保存状态",
            ResidueKind::Caches => "缓存",
            ResidueKind::LaunchItems => "启动项",
            ResidueKind::CrashLogs => "崩溃日志",
            ResidueKind::Other => "其他文件",
        }
    }
}

/// 一类残留：若干路径 + 总大小
#[derive(Debug, Clone)]
pub struct Residue {
    pub kind: ResidueKind,
    pub paths: Vec<PathBuf>,
    pub size: u64,
}

/// 当前进程能否读取沙盒容器目录（不能则大概率缺「完全磁盘访问权限」，
/// 残留扫描会漏掉容器/WebKit 等受保护路径）
pub fn full_disk_access_granted() -> bool {
    let Some(home) = dirs::home_dir() else { return true };
    match std::fs::read_dir(home.join("Library/Containers")) {
        Ok(_) => true,
        Err(e) => e.kind() != std::io::ErrorKind::PermissionDenied,
    }
}

/// 扫描一个应用的所有残留候选
pub fn scan(bundle_id: &str, name: &str, app_path: &Path) -> Vec<Residue> {
    let mut kinds: Vec<(ResidueKind, Vec<PathBuf>)> = Vec::new();
    let mut push = |kind: ResidueKind, p: PathBuf| {
        if !p.exists() {
            return;
        }
        if let Some(entry) = kinds.iter_mut().find(|(k, _)| *k == kind) {
            if !entry.1.contains(&p) {
                entry.1.push(p);
            }
        } else {
            kinds.push((kind, vec![p]));
        }
    };

    let Some(home) = dirs::home_dir() else { return Vec::new() };
    let lib = home.join("Library");
    let bid = bundle_id.trim();
    let app_name = name.trim();

    // .app 本体
    if app_path.exists() {
        push(ResidueKind::AppFiles, app_path.to_path_buf());
    }

    // 按应用名与 bundle id 双重匹配
    if !app_name.is_empty() {
        push(ResidueKind::AppSupport, lib.join("Application Support").join(app_name));
        push(ResidueKind::Caches, lib.join("Caches").join(app_name));
        push(ResidueKind::WebKit, lib.join("WebKit").join(app_name));
    }
    if !bid.is_empty() {
        push(ResidueKind::AppSupport, lib.join("Application Support").join(bid));
        push(ResidueKind::Caches, lib.join("Caches").join(bid));
        push(ResidueKind::WebKit, lib.join("WebKit").join(bid));
        push(ResidueKind::Container, lib.join("Containers").join(bid));
        push(ResidueKind::Preferences, lib.join("Preferences").join(format!("{bid}.plist")));
        push(ResidueKind::Preferences, lib.join("Preferences").join("ByHost").join(format!("{bid}.plist")));
        push(
            ResidueKind::SavedState,
            lib.join("Saved Application State").join(format!("{bid}.savedState")),
        );
        push(ResidueKind::Other, lib.join("Application Scripts").join(bid));
        push(ResidueKind::Other, lib.join("HTTPStorages").join(bid));
        push(ResidueKind::Other, lib.join("Cookies").join(format!("{bid}.binarycookies")));
        // 容器组：group.<bundle_id> 或 <team-id>.<bundle_id>
        if let Ok(rd) = std::fs::read_dir(lib.join("Group Containers")) {
            for e in rd.flatten() {
                if let Some(n) = e.file_name().to_str() {
                    if n.ends_with(bid) {
                        push(ResidueKind::GroupContainer, e.path());
                    }
                }
            }
        }
    }

    // 崩溃日志：DiagnosticReports/<name>-*.ips / .crash / .hang
    if !app_name.is_empty() {
        if let Ok(rd) = std::fs::read_dir(lib.join("Logs/DiagnosticReports")) {
            for e in rd.flatten() {
                if let Some(n) = e.file_name().to_str() {
                    if n.starts_with(app_name)
                        && (n.ends_with(".ips") || n.ends_with(".crash") || n.ends_with(".hang"))
                    {
                        push(ResidueKind::CrashLogs, e.path());
                    }
                }
            }
        }
    }

    // 启动项：LaunchAgents / LaunchDaemons 里引用了应用路径或 bundle id
    for dir in [lib.join("LaunchAgents"), PathBuf::from("/Library/LaunchAgents"), PathBuf::from("/Library/LaunchDaemons")] {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("plist") {
                    continue;
                }
                if launch_item_refs(&p, app_path, bid, app_name) {
                    push(ResidueKind::LaunchItems, p);
                }
            }
        }
    }

    kinds
        .into_iter()
        .map(|(kind, paths)| {
            let size = paths.iter().map(|p| super::scanner::AppScanner::calculate_dir_size(p)).sum();
            Residue { kind, paths, size }
        })
        .filter(|r| !r.paths.is_empty())
        .collect()
}

/// 启动项 plist 是否引用了该应用（程序路径 / bundle id / 应用名）
fn launch_item_refs(plist: &Path, app_path: &Path, bundle_id: &str, app_name: &str) -> bool {
    let Some(content) = std::fs::read_to_string(plist).ok() else { return false };
    let app_path = app_path.to_string_lossy();
    let app_path = app_path.trim_end_matches('/');
    let lower = content.to_lowercase();
    if !app_path.is_empty() && lower.contains(&app_path.to_lowercase()) {
        return true;
    }
    if !bundle_id.is_empty() && lower.contains(&bundle_id.to_lowercase()) {
        return true;
    }
    !app_name.is_empty() && lower.contains(&app_name.to_lowercase())
}
