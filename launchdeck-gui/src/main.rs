//! LaunchDeck - 纯 Rust Launchpad 复刻版

mod app;
mod icons;
mod native;
mod wallpaper;
mod layout;
mod layout_store;
mod orig_layout;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    // 单实例：已有实例在运行时直接激活它并退出（两个实例会争抢同一个全局热键）
    if let Some(_pid) = native::other_instance_pid() {
        eprintln!("LaunchDeck 已在运行");
        std::process::exit(0);
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    if let Err(e) = launchdeck_core::init() {
        eprintln!("核心库初始化失败: {e}");
    }

    // 窗口创建时就按当前模式一次到位，避免启动时先 1024×700 再跳到全屏
    let st = launchdeck_core::get_settings();
    let (sw, sh) = native::main_screen_size().unwrap_or((2048.0, 1152.0));
    let (w, h) = if st.display_mode == launchdeck_core::DisplayMode::Window {
        (1200.0, 800.0)
    } else {
        (sw, sh)
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([w as f32, h as f32])
            .with_position([((sw - w) / 2.0) as f32, ((sh - h) / 2.0) as f32])
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true)
            .with_title("LaunchDeck"),
        ..Default::default()
    };

    eframe::run_native(
        "LaunchDeck",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            setup_style(&cc.egui_ctx);
            Ok(Box::new(app::LaunchDeckApp::new(cc)))
        }),
    )
}

/// 注入 SF Pro 与中文字体，观感接近原生 macOS
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 拉丁字形优先使用 SF Pro
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        fonts
            .font_data
            .insert("sf".to_owned(), egui::FontData::from_owned(bytes).into());
        if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            f.insert(0, "sf".to_owned());
        }
    }

    // CJK 回退
    let candidates = [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
    ];
    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else { continue };
        fonts
            .font_data
            .insert("cjk".to_owned(), egui::FontData::from_owned(bytes).into());
        if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            f.push("cjk".to_owned());
        }
        if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            f.push("cjk".to_owned());
        }
        break;
    }
    ctx.set_fonts(fonts);
}

/// 统一的圆角、间距与控件配色
fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.menu_margin = egui::Margin::same(8.0);
    style.spacing.interact_size.y = 26.0;

    let w = &mut style.visuals.widgets;
    w.inactive.rounding = egui::Rounding::same(8.0);
    w.hovered.rounding = egui::Rounding::same(8.0);
    w.active.rounding = egui::Rounding::same(8.0);
    w.open.rounding = egui::Rounding::same(8.0);
    w.inactive.bg_fill = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14);
    w.hovered.bg_fill = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 28);
    w.active.bg_fill = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 40);
    w.inactive.weak_bg_fill = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 10);
    w.hovered.weak_bg_fill = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 24);
    w.active.weak_bg_fill = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 36);
    style.visuals.window_rounding = egui::Rounding::same(16.0);
    style.visuals.window_fill = egui::Color32::from_rgba_unmultiplied(32, 32, 34, 248);
    style.visuals.panel_fill = egui::Color32::from_rgba_unmultiplied(32, 32, 34, 248);
    style.visuals.window_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(26));
    style.visuals.menu_rounding = egui::Rounding::same(10.0);
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 12.0),
        blur: 32.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(90),
    };
    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 6.0),
        blur: 18.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(80),
    };
    ctx.set_style(style);
}
