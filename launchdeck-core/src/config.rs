use dirs;
use crate::{error::*, model::*};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 数据目录名
const APP_DIR: &str = "LaunchDeck";
/// 改名前的数据目录名（用于一次性迁移）
const LEGACY_DIR: &str = "LaunchOS";

/// 配置文件路径
fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(APP_DIR)
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// 从旧名字的数据目录迁移设置与布局（只补齐缺失的文件，不覆盖新数据）
fn migrate_legacy() {
    let Some(base) = dirs::config_dir() else { return };
    let old = base.join(LEGACY_DIR);
    if !old.is_dir() {
        return;
    }
    let new = base.join(APP_DIR);
    for name in ["settings.json", "layout.json"] {
        let src = old.join(name);
        let dst = new.join(name);
        if src.exists() && !dst.exists() {
            let _ = std::fs::copy(&src, &dst);
        }
    }
}

/// 全局设置实例
static SETTINGS: Lazy<RwLock<AppSettings>> = Lazy::new(|| RwLock::new(AppSettings::default()));

/// 初始化配置系统
pub fn init() -> Result<()> {
    std::fs::create_dir_all(config_dir())?;
    migrate_legacy();
    load_settings()?;
    Ok(())
}

/// 加载设置
fn load_settings() -> Result<()> {
    let path = settings_path();
    if path.exists() {
        let data = std::fs::read_to_string(&path)?;
        let settings: AppSettings = serde_json::from_str(&data)?;
        *SETTINGS.write() = settings;
    }
    Ok(())
}

/// 保存设置
pub fn save_settings(settings: &AppSettings) -> Result<()> {
    let path = settings_path();
    let data = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, data)?;
    *SETTINGS.write() = settings.clone();
    Ok(())
}

/// 获取当前设置
pub fn get_settings() -> AppSettings {
    SETTINGS.read().clone()
}

/// 更新设置
pub fn update_settings<F>(f: F) -> Result<()>
where
    F: FnOnce(&mut AppSettings),
{
    let mut settings = SETTINGS.write();
    f(&mut settings);
    save_settings(&settings)
}
