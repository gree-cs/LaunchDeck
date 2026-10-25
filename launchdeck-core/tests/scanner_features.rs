//! 扫描器扩展功能的端到端测试：自定义来源、替身收集、bundle id 读取。

use launchdeck_core::AppScanner;
use std::path::PathBuf;

/// 在 ~/Applications 放一个指向系统计算器的符号链接 .app（替身的一种），
/// 默认扫描应能收集到且标记 is_alias
#[test]
fn scan_collects_alias_app() {
    let home = dirs::home_dir().unwrap();
    let apps_dir = home.join("Applications");
    std::fs::create_dir_all(&apps_dir).unwrap();
    let alias = apps_dir.join("LaunchDeckAliasE2E.app");
    let target = PathBuf::from("/System/Applications/Calculator.app");
    if !target.exists() {
        // 系统没有计算器（不太可能）就跳过
        return;
    }
    let _ = std::fs::remove_file(&alias);
    std::os::unix::fs::symlink(&target, &alias).unwrap();

    let list = AppScanner::default().scan().unwrap();
    let hit = list
        .iter()
        .find(|a| a.path == alias)
        .expect("替身 .app 应被扫描收集");
    assert!(hit.is_alias, "替身条目应带 is_alias 标记");
    // 别名条目按路径去重，不该与真实 Calculator 冲突（bundle id 为空）
    let _ = std::fs::remove_file(&alias);
}

#[test]
fn read_bundle_id_of_system_app() {
    let calc = PathBuf::from("/System/Applications/Calculator.app");
    if !calc.exists() {
        return;
    }
    let bid = AppScanner::read_bundle_id(&calc).expect("应能读取 Calculator 的 bundle id");
    assert_eq!(bid, "com.apple.calculator");
}

/// 指定自定义根目录扫描（AppScanner::new）
#[test]
fn scan_with_custom_roots() {
    let tmp = std::env::temp_dir().join("launchdeck-scan-roots-e2e");
    let _ = std::fs::remove_dir_all(&tmp);
    let app = tmp.join("FakeApp.app/Contents/MacOS");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("FakeApp"), b"#!/bin/sh\n").unwrap();

    let scanner = AppScanner::new(vec![tmp.clone()]);
    let list = scanner.scan().unwrap();
    assert!(
        list.iter().any(|a| a.path == tmp.join("FakeApp.app")),
        "自定义根目录下的 .app 应被扫描到"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
