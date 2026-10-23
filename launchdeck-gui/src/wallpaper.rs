//! 桌面壁纸模糊背景（模拟 Launchpad 的模糊壁纸效果）

use egui::ColorImage;

/// 通过 NSWorkspace 获取当前桌面壁纸路径（无需权限）
pub fn desktop_wallpaper_path() -> Option<String> {
    crate::native::wallpaper_path()
}

/// 后台加载 + 模糊，返回处理好的图像
pub fn load_blurred(path: &str) -> Option<ColorImage> {
    let img = match image::open(path) {
        Ok(img) => img.to_rgba8(),
        Err(_) => {
            // HEIC 等格式先用系统 sips 转 PNG
            let out = std::env::temp_dir().join("launchdeck-wallpaper.png");
            let ok = std::process::Command::new("/usr/bin/sips")
                .args(["-s", "format", "png", "--resampleWidth", "1600"])
                .arg(path)
                .arg("--out")
                .arg(&out)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !ok {
                return None;
            }
            image::open(&out).ok()?.to_rgba8()
        }
    };

    // 毛玻璃：在较高分辨率下做高斯模糊，再平滑放大。
    // 640×360 上 σ=6.4（等效 2048 宽下约 σ≈20），保留壁纸的可辨认轮廓。
    let small = image::imageops::resize(&img, 640, 360, image::imageops::FilterType::Triangle);
    let blurred = image::imageops::blur(&small, 5.0);
    let mut out = image::imageops::resize(
        &blurred,
        1600,
        900,
        image::imageops::FilterType::CatmullRom,
    );

    // 提一点饱和度 + 轻微压暗，观感更接近原版
    for p in out.pixels_mut() {
        let [r, g, b, _] = p.0;
        let (rf, gf, bf) = (r as f32, g as f32, b as f32);
        let lum = 0.299 * rf + 0.587 * gf + 0.114 * bf;
        let sat = 1.18;
        let f = |c: f32| ((lum + (c - lum) * sat) * 0.99).clamp(0.0, 255.0) as u8;
        *p = image::Rgba([f(rf), f(gf), f(bf), 255]);
    }

    let (w, h) = out.dimensions();
    Some(ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        &out.into_raw(),
    ))
}

/// 生成圆角版本（窗口模式用）：放大/裁剪到 w×h，并把四角裁成透明
pub fn rounded_copy(img: &egui::ColorImage, w: u32, h: u32, radius: f32) -> egui::ColorImage {
    let (sw, sh) = (img.size[0] as u32, img.size[1] as u32);
    let src = image::RgbaImage::from_raw(sw, sh, img.as_raw().to_vec()).unwrap_or_else(|| {
        image::RgbaImage::new(sw, sh)
    });
    // 等比铺满
    let scale = (w as f32 / sw as f32).max(h as f32 / sh as f32);
    let (rw, rh) = ((sw as f32 * scale).ceil() as u32, (sh as f32 * scale).ceil() as u32);
    let resized = image::imageops::resize(&src, rw, rh, image::imageops::FilterType::CatmullRom);
    let ox = (rw.saturating_sub(w)) / 2;
    let oy = (rh.saturating_sub(h)) / 2;
    let mut out = image::imageops::crop_imm(&resized, ox, oy, w, h).to_image();

    for (x, y, px) in out.enumerate_pixels_mut() {
        let dx = if (x as f32) < radius {
            radius - x as f32
        } else if (x as f32) > w as f32 - radius {
            x as f32 - (w as f32 - radius)
        } else {
            0.0
        };
        let dy = if (y as f32) < radius {
            radius - y as f32
        } else if (y as f32) > h as f32 - radius {
            y as f32 - (h as f32 - radius)
        } else {
            0.0
        };
        if dx > 0.0 && dy > 0.0 {
            let d = (dx * dx + dy * dy).sqrt();
            let a = ((radius - d) / 1.5).clamp(0.0, 1.0);
            px.0[3] = (px.0[3] as f32 * a) as u8;
        }
    }
    let (w, h) = out.dimensions();
    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], out.as_raw())
}

/// 文件夹背板用的重模糊版本（更糊、更暗）
pub fn heavy_blur(img: &egui::ColorImage) -> egui::ColorImage {
    let (w, h) = (img.size[0] as u32, img.size[1] as u32);
    let src = image::RgbaImage::from_raw(w, h, img.as_raw().to_vec())
        .unwrap_or_else(|| image::RgbaImage::new(w, h));
    let small = image::imageops::resize(&src, 320, 180, image::imageops::FilterType::Triangle);
    let blurred = image::imageops::blur(&small, 22.0);
    let out = image::imageops::resize(&blurred, 1200, 675, image::imageops::FilterType::CatmullRom);
    egui::ColorImage::from_rgba_unmultiplied([1200, 675], out.as_raw())
}
