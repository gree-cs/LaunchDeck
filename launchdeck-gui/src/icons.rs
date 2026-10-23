//! 应用图标加载
//!
//! 通过随包分发的 `icongen` 辅助程序（NSWorkspace）批量导出 PNG，
//! 支持 Assets.car 里的图标（sips 无法读取）。带 mtime 缓存，二次启动无需重导。

use egui::{ColorImage, TextureHandle, TextureOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Default)]
pub struct IconCache {
    map: HashMap<String, Option<TextureHandle>>,
    files: HashMap<String, PathBuf>,
}

impl IconCache {
    /// 扫描完成后注入 path -> png 映射
    pub fn set_manifest(&mut self, files: HashMap<String, PathBuf>) {
        self.files = files;
        self.map.clear();
    }

    pub fn get(&mut self, ctx: &egui::Context, app_path: &str) -> Option<TextureHandle> {
        if let Some(cached) = self.map.get(app_path) {
            return cached.clone();
        }
        let tex = self
            .files
            .get(app_path)
            .and_then(|p| load_png(ctx, p))
            .or_else(|| load_with_sips(ctx, app_path));
        self.map.insert(app_path.to_string(), tex.clone());
        tex
    }
}

fn load_png(ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
    let mut img = image::open(path).ok()?.to_rgba8();
    if img.width() > 256 || img.height() > 256 {
        img = image::imageops::resize(&img, 256, 256, image::imageops::FilterType::Lanczos3);
    }
    let (w, h) = img.dimensions();
    let color = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img.into_raw());
    Some(ctx.load_texture(
        format!("icon:{}", path.display()),
        color,
        TextureOptions::LINEAR,
    ))
}

/// 兜底：从 .icns 用 sips 转换（辅助程序缺失时）
fn load_with_sips(ctx: &egui::Context, app_path: &str) -> Option<TextureHandle> {
    let contents = Path::new(app_path).join("Contents");
    let mut icns = None;
    if let Ok(txt) = std::fs::read_to_string(contents.join("Info.plist")) {
        if let Some(pos) = txt.find("<key>CFBundleIconFile</key>") {
            let after = &txt[pos..];
            if let Some(s) = after.find("<string>") {
                let mut name = after[s + 8..].split("</string>").next()?.trim().to_string();
                if !name.ends_with(".icns") {
                    name.push_str(".icns");
                }
                let p = contents.join("Resources").join(name);
                if p.exists() {
                    icns = Some(p);
                }
            }
        }
    }
    if icns.is_none() {
        for n in ["AppIcon.icns", "app.icns", "icon.icns"] {
            let p = contents.join("Resources").join(n);
            if p.exists() {
                icns = Some(p);
                break;
            }
        }
    }
    let icns = icns?;
    let out = std::env::temp_dir().join(format!(
        "launchdeck-icns-{:x}.png",
        fnv1a(app_path.as_bytes())
    ));
    let ok = Command::new("/usr/bin/sips")
        .args(["-s", "format", "png", "-z", "128", "128"])
        .arg(&icns)
        .arg("--out")
        .arg(&out)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        return None;
    }
    load_png(ctx, &out)
}

/// 从旧缓存目录（LaunchOS）一次性复制图标缓存，避免改名后首次启动重新导出
static MIGRATE_CACHE: std::sync::Once = std::sync::Once::new();

pub fn cache_dir() -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join("LaunchDeck")
        .join("icons-v2");
    MIGRATE_CACHE.call_once(|| {
        if dir.exists() {
            return;
        }
        let Some(base) = dirs::cache_dir() else { return };
        let old = base.join("LaunchOS").join("icons-v2");
        if old.is_dir() {
            let _ = copy_dir(&old, &dir);
        }
    });
    dir
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Entry {
    file: String,
    mtime: u64,
}

/// 批量导出图标。返回 app path -> png 路径。
/// 在后台线程调用（会启动一次 `icongen` 子进程）。
pub fn generate(paths: &[(String, u64)]) -> HashMap<String, PathBuf> {
    let dir = cache_dir();
    let _ = std::fs::create_dir_all(&dir);
    let manifest_path = dir.join("manifest.json");

    let mut manifest: HashMap<String, Entry> = std::fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let stale: Vec<&(String, u64)> = paths
        .iter()
        .filter(|(p, mt)| {
            manifest
                .get(p)
                .map(|e| e.mtime != *mt || !dir.join(&e.file).exists())
                .unwrap_or(true)
        })
        .collect();

    if !stale.is_empty() {
        if let Some(helper) = helper_path() {
            let mut cmd = Command::new(helper);
            cmd.arg(&dir);
            for (p, _) in &stale {
                cmd.arg(p);
            }
            if cmd.output().map(|o| o.status.success()).unwrap_or(false) {
                if let Ok(txt) = std::fs::read_to_string(dir.join("manifest.json")) {
                    if let Ok(produced) = serde_json::from_str::<HashMap<String, String>>(&txt) {
                        for (p, f) in produced {
                            let mt = paths
                                .iter()
                                .find(|(x, _)| x == &p)
                                .map(|(_, m)| *m)
                                .unwrap_or(0);
                            manifest.insert(p, Entry { file: f, mtime: mt });
                        }
                    }
                }
            }
        }
        if let Ok(txt) = serde_json::to_string(&manifest) {
            let _ = std::fs::write(&manifest_path, txt);
        }
    }

    manifest
        .into_iter()
        .filter(|(_, e)| dir.join(&e.file).exists())
        .map(|(p, e)| (p, dir.join(e.file)))
        .collect()
}

fn helper_path() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            cands.push(d.join("icongen"));
            cands.push(d.join("../Resources/icongen"));
        }
    }
    for d in ["target/release", "target/debug"] {
        cands.push(PathBuf::from(d).join("icongen"));
    }
    cands.into_iter().find(|p| p.exists())
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ---------------------------------------------------------------- SF Symbols

use std::sync::{Mutex, OnceLock};

static SYMBOLS: OnceLock<Mutex<HashMap<String, Option<TextureHandle>>>> = OnceLock::new();

/// 加载构建时渲染好的 SF Symbol（白色字形 + alpha），按名字缓存
pub fn symbol(ctx: &egui::Context, name: &str) -> Option<TextureHandle> {
    let cache = SYMBOLS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().ok().and_then(|m| m.get(name).cloned()) {
        return hit;
    }
    let tex = load_symbol_png(ctx, name);
    if let Ok(mut m) = cache.lock() {
        m.insert(name.to_string(), tex.clone());
    }
    tex
}

fn load_symbol_png(ctx: &egui::Context, name: &str) -> Option<TextureHandle> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?.join("Resources").join("symbols");
    let file = format!("sym_{}.png", name.replace('.', "_"));
    let mut img = image::open(dir.join(file)).ok()?.to_rgba8();
    // 模板图：统一染成纯白，颜色交给绘制时 tint
    for p in img.pixels_mut() {
        let a = p.0[3];
        p.0 = [255, 255, 255, a];
    }
    let (w, h) = img.dimensions();
    let color = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img.into_raw());
    Some(ctx.load_texture(
        format!("sym:{name}"),
        color,
        TextureOptions::LINEAR,
    ))
}

// ---------------------------------------------------------------- 资源 PNG

static ASSETS: OnceLock<Mutex<HashMap<String, Option<TextureHandle>>>> = OnceLock::new();

/// 从 bundle 的 Resources/<sub>/<name>.png 加载（带缓存）
pub fn asset(ctx: &egui::Context, sub: &str, name: &str) -> Option<TextureHandle> {
    let key = format!("{sub}/{name}");
    let cache = ASSETS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().ok().and_then(|m| m.get(&key).cloned()) {
        return hit;
    }
    let tex = (|| {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?.parent()?.join("Resources").join(sub);
        let img = image::open(dir.join(format!("{name}.png"))).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let color = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img.into_raw());
        Some(ctx.load_texture(key.clone(), color, TextureOptions::LINEAR))
    })();
    if let Ok(mut m) = cache.lock() {
        m.insert(key, tex.clone());
    }
    tex
}
