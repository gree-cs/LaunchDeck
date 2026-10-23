use crate::error::*;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use sha2::{Digest, Sha256};

/// 计算文件/目录 SHA256
pub fn sha256_file(path: &std::path::Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// 计算目录大小
pub fn dir_size(path: &std::path::Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// 格式化字节大小
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}

/// 格式化持续时间
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {:02}h", secs / 86400, (secs % 86400) / 3600)
    }
}

/// 扩展名处理
pub fn extension(path: &std::path::Path) -> Option<String> {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
}

/// 规范化路径
pub fn normalize_path(path: &std::path::Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// 获取文件创建时间
pub fn created_time(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.created().ok()
}

/// 获取文件修改时间
pub fn modified_time(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// 判断是否为应用包
pub fn is_app_bundle(path: &std::path::Path) -> bool {
    path.extension().map(|e| e == "app").unwrap_or(false)
        && path.join("Contents/Info.plist").exists()
}

/// 获取应用版本号
pub fn get_app_version(path: &std::path::Path) -> Option<String> {
    let plist_path = path.join("Contents/Info.plist");
    if !plist_path.exists() {
        return None;
    }

    // 简化：实际应解析 plist
    let content = std::fs::read_to_string(&plist_path).ok()?;
    // 简单查找 CFBundleShortVersionString
    if let Some(start) = content.find("CFBundleShortVersionString") {
        let after = &content[start..];
        if let Some(start) = after.find('>') {
            let after = &after[start + 1..];
            if let Some(end) = after.find('<') {
                return Some(after[..end].to_string());
            }
        }
    }
    None
}

/// 获取 bundle identifier
pub fn get_bundle_id(path: &std::path::Path) -> Option<String> {
    let plist_path = path.join("Contents/Info.plist");
    if !plist_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&plist_path).ok()?;
    if let Some(start) = content.find("CFBundleIdentifier") {
        let after = &content[start..];
        if let Some(start) = after.find('>') {
            let after = &after[start + 1..];
            if let Some(end) = after.find('<') {
                return Some(after[..end].to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(std::time::Duration::from_secs(30)), "30s");
        assert_eq!(format_duration(std::time::Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_duration(std::time::Duration::from_secs(3661)), "1h 01m");
    }
}
