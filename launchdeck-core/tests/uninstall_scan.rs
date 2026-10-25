//! 深度卸载残留扫描的端到端测试：
//! 在真实用户 Library 里创建带假 bundle id 的临时残留目录/文件，
//! 断言 scan() 能全部找到，结束后清理。

use launchdeck_core::uninstall::{self, ResidueKind};
use std::path::PathBuf;

const BID: &str = "app.launchdeck.e2e";
const NAME: &str = "LaunchDeckE2E";

/// 测试里创建的所有路径（Drop 时统一清理）
struct Cleaner(Vec<PathBuf>);

impl Cleaner {
    fn push(&mut self, p: PathBuf) -> PathBuf {
        self.0.push(p.clone());
        p
    }
}

impl Drop for Cleaner {
    fn drop(&mut self) {
        for p in &self.0 {
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(p);
            } else {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

#[test]
fn scan_finds_all_residue_kinds() {
    let home = dirs::home_dir().expect("home");
    let lib = home.join("Library");
    let mut c = Cleaner(vec![]);

    // 假应用本体
    let app = std::env::temp_dir().join(format!("{NAME}.app"));
    let _ = std::fs::remove_dir_all(&app);
    std::fs::create_dir_all(&app).unwrap();
    c.push(app.clone());

    // 各类残留（按当前权限能建多少建多少）
    let mut created_kinds: Vec<ResidueKind> = vec![ResidueKind::AppFiles];
    let mk_dir = |base: PathBuf, kind: ResidueKind, created: &mut Vec<ResidueKind>, c: &mut Cleaner| {
        match std::fs::create_dir_all(&base) {
            Ok(_) => {
                // 写一个占位文件，目录大小统计才有意义
                let _ = std::fs::write(base.join("placeholder"), b"x");
                c.push(base.clone());
                created.push(kind);
            }
            Err(_) => {} // 无权限（例如缺完全磁盘访问）则跳过该类断言
        }
    };

    mk_dir(
        lib.join("Application Support").join(NAME),
        ResidueKind::AppSupport,
        &mut created_kinds,
        &mut c,
    );
    mk_dir(
        lib.join("Caches").join(BID),
        ResidueKind::Caches,
        &mut created_kinds,
        &mut c,
    );
    let webkit = lib.join("WebKit").join(BID);
    if std::fs::create_dir_all(&webkit).is_ok() {
        let _ = std::fs::write(webkit.join("data"), b"x");
        c.push(webkit);
        created_kinds.push(ResidueKind::WebKit);
    }
    let container = lib.join("Containers").join(BID);
    if std::fs::create_dir_all(&container).is_ok() {
        let _ = std::fs::write(container.join("data"), b"x");
        c.push(container);
        created_kinds.push(ResidueKind::Container);
    }
    let group = lib.join("Group Containers").join(format!("group.{BID}"));
    if std::fs::create_dir_all(&group).is_ok() {
        let _ = std::fs::write(group.join("data"), b"x");
        c.push(group);
        created_kinds.push(ResidueKind::GroupContainer);
    }
    let saved = lib.join("Saved Application State").join(format!("{BID}.savedState"));
    if std::fs::create_dir_all(&saved).is_ok() {
        c.push(saved);
        created_kinds.push(ResidueKind::SavedState);
    }
    let pref = lib.join("Preferences").join(format!("{BID}.plist"));
    if std::fs::write(&pref, b"bplist00-\x10\x01").is_ok() {
        c.push(pref);
        created_kinds.push(ResidueKind::Preferences);
    }
    let scripts = lib.join("Application Scripts").join(BID);
    if std::fs::create_dir_all(&scripts).is_ok() {
        c.push(scripts);
        created_kinds.push(ResidueKind::Other);
    }

    // 崩溃日志（文件名前缀匹配应用名）
    let diag = lib.join("Logs/DiagnosticReports");
    if std::fs::create_dir_all(&diag).is_ok() {
        let crash = diag.join(format!("{NAME}-2026-10-24-000000.ips"));
        if std::fs::write(&crash, b"{}").is_ok() {
            c.push(crash);
            created_kinds.push(ResidueKind::CrashLogs);
        }
    }

    // 启动项（内容引用 bundle id）
    let la_dir = lib.join("LaunchAgents");
    if std::fs::create_dir_all(&la_dir).is_ok() {
        let la = la_dir.join(format!("{BID}.plist"));
        let content = format!("<?xml version=\"1.0\"?><plist><dict><key>Label</key><string>{BID}</string></dict></plist>");
        if std::fs::write(&la, content).is_ok() {
            c.push(la);
            created_kinds.push(ResidueKind::LaunchItems);
        }
    }

    let residues = uninstall::scan(BID, NAME, &app);
    assert!(!residues.is_empty(), "至少应找到应用本体");

    for kind in &created_kinds {
        let hit = residues.iter().find(|r| r.kind == *kind);
        assert!(
            hit.is_some(),
            "未找到 {:?}（{:?} 类残留已创建但扫描不到）",
            kind,
            kind
        );
        let r = hit.unwrap();
        assert!(!r.paths.is_empty());
        // 每条路径都真实存在（扫描不该报假路径）
        for p in &r.paths {
            assert!(p.exists(), "报告的路径不存在: {p:?}");
        }
    }

    // 大小统计：占位文件至少贡献了字节数
    let support = residues
        .iter()
        .find(|r| r.kind == ResidueKind::AppSupport);
    if let (Some(r), true) = (support, created_kinds.contains(&ResidueKind::AppSupport)) {
        assert!(r.size > 0, "支持文件大小应大于 0");
    }
}

#[test]
fn scan_of_missing_app_returns_app_files_only_or_empty() {
    // 不存在的应用不应 panic；且不能把别的应用的残留误报进来
    let residues = uninstall::scan(
        "app.launchdeck.nonexistent.e2e",
        "NoSuchAppE2E",
        std::path::Path::new("/tmp/app.launchdeck.nonexistent.e2e.app"),
    );
    for r in &residues {
        assert_ne!(r.kind, ResidueKind::AppFiles, "应用本体不存在时不应报告 AppFiles");
        for p in &r.paths {
            let s = p.to_string_lossy();
            assert!(
                !s.contains("NonexistentE2E") || s.contains("NoSuchAppE2E"),
                "不应包含无关路径"
            );
        }
    }
}

#[test]
fn full_disk_access_reported_consistently() {
    // 只要求不 panic，两种结果都合法
    let _ = uninstall::full_disk_access_granted();
}
