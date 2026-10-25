//! 1:1 复刻原版 LaunchOS 的界面。
//!
//! 尺寸依据对原版 2.4.0 在 2048×1152 下的逐像素量取：
//! - 搜索胶囊 244×40，中心 y=61.5（上沿 41.5），内容基线中心 65.5
//! - 7 列网格，列距 251.6（首列中心 269），图标可见 105（贴图 130，四周约 9.5% 透明边）
//! - 行距 180.4，首行图标中心 y≈192.5，标签中心距图标底 12
//! - 页点中心 y=1043.5，间距 20，实心点直径 8
//! - 文件夹预览 3×3；展开面板 x 144..1904、y 117..1022，行距 178.7，圆角 24
//! - 设置窗口 400×610

use crate::app::{LaunchDeckApp, Tab};
use crate::layout::{Item, SortMode};
use eframe::egui;
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, Vec2};
use launchdeck_core::{
    BackgroundType, DisplayMode, HotCorner, IconStyle, ReturnHome, ScreenChoice,
};

const GREEN: Color32 = Color32::from_rgb(48, 209, 88);
const BLUE: Color32 = Color32::from_rgb(10, 132, 255);

struct Pal {
    dark: bool,
    text: Color32,
    sub: Color32,
    panel: Color32,
    panel_border: Color32,
    control: Color32,
    separator: Color32,
}

fn pal(app: &LaunchDeckApp) -> Pal {
    if app.is_dark() {
        Pal {
            dark: true,
            text: Color32::from_rgb(242, 242, 247),
            sub: Color32::from_rgb(142, 142, 147),
            panel: Color32::from_rgba_unmultiplied(28, 28, 30, 246),
            panel_border: Color32::from_rgba_unmultiplied(255, 255, 255, 28),
            control: Color32::from_rgba_unmultiplied(255, 255, 255, 18),
            separator: Color32::from_rgba_unmultiplied(255, 255, 255, 24),
        }
    } else {
        Pal {
            dark: false,
            text: Color32::from_rgb(28, 28, 30),
            sub: Color32::from_rgb(110, 110, 115),
            panel: Color32::from_rgba_unmultiplied(246, 246, 248, 250),
            panel_border: Color32::from_rgba_unmultiplied(0, 0, 0, 24),
            control: Color32::from_rgba_unmultiplied(0, 0, 0, 12),
            separator: Color32::from_rgba_unmultiplied(0, 0, 0, 20),
        }
    }
}

pub fn draw(app: &mut LaunchDeckApp, ctx: &egui::Context) {
    // 淡出过程中仍要绘制；完全隐藏后跳过（窗口保持透明）
    if !app.visible && !app.closing {
        return;
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::none())
        .show(ctx, |ui| {
            ui.set_opacity(if app.visible { ease_out(app.show_anim) } else { 0.0 });
            // 只有启动台页需要不透明背景；设置/卸载页保持透明，
            // 让真实桌面透出来（与原版「设置是浮在桌面上的独立窗口」一致）。
            if app.tab == Tab::Launchpad {
                paint_background(app, ui);
                // 网格始终绘制：文件夹是浮在网格之上的浮层，
                // 这样打开/关闭文件夹时背景不会突然消失再出现（闪烁来源）。
                if app.search.trim().is_empty() && app.category_filter.is_some() {
                    category_results(app, ui);
                } else if app.search.trim().is_empty() {
                    if app.layout.browsing_style == crate::layout::BrowsingStyle::Scroll {
                        scroll_view(app, ui);
                    } else {
                        grid_page(app, ui);
                    }
                } else {
                    search_results(app, ui);
                }
                if app.open_folder.is_none() {
                    search_bar(app, ui);
                    if app.layout.browsing_style == crate::layout::BrowsingStyle::Paged {
                        page_dots(app, ui);
                    }
                    // 最后注册，保证盖过网格的整屏交互
                    if app.search.trim().is_empty() {
                        let r = search_rect(ui.max_rect());
                        let bc = egui::pos2(r.max.x - 24.0, r.min.y + 24.5);
                        let resp = ui.interact(
                            Rect::from_center_size(bc, egui::vec2(26.0, 26.0)),
                            egui::Id::new("search-menu"),
                            Sense::click(),
                        );
                        if resp.clicked() {
                            app.main_menu_open = !app.main_menu_open;
                            app.main_menu_types_open = false;
                            app.menu_open_at_frame = ui.ctx().frame_nr();
                        }
                    }
                }
            }
        });

    match app.tab {
        Tab::Settings => settings_window(app, ctx),
        Tab::Uninstall => uninstall_window(app, ctx),
        _ => {}
    }

    // 搜索栏「⋯」打开的主菜单（与空白处右键菜单同一套内容）
    if app.main_menu_open
        && app.visible
        && !app.closing
        && app.tab == Tab::Launchpad
        && app.open_folder.is_none()
    {
        draw_main_menu(app, ctx);
    }

    folder_view(app, ctx);
    confirm_dialog(app, ctx);
    uninstall_detail_dialog(app, ctx);
    tidy_pages_dialog(app, ctx);
    app_rename_overlay(app, ctx);
}

/// 应用重命名浮层（右键「重命名」）
fn app_rename_overlay(app: &mut LaunchDeckApp, ctx: &egui::Context) {
    if app.renaming_app.is_none() {
        return;
    }
    let screen = ctx.screen_rect();
    let p = pal(app);
    let w = 320.0;
    let h = 118.0;
    let pos = screen.center() - egui::vec2(w, h) / 2.0;
    let mut commit: Option<String> = None;

    egui::Area::new(egui::Id::new("app-rename"))
        .fixed_pos(pos)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(p.panel)
                .rounding(12.0)
                .stroke(Stroke::new(1.0_f32, p.panel_border))
                .inner_margin(egui::Margin::same(16.0))
                .show(ui, |ui| {
                    ui.set_width(w - 32.0);
                    ui.label(egui::RichText::new("重命名").size(14.0).strong());
                    ui.add_space(4.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut app.edit_text)
                            .id(egui::Id::new("app-rename-box"))
                            .desired_width(w - 64.0),
                    );
                    if app.focus_editing {
                        resp.request_focus();
                        app.focus_editing = false;
                    }
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    if enter {
                        commit = Some(app.edit_text.trim().to_string());
                    }
                    if esc {
                        app.renaming_app = None;
                        app.edit_text.clear();
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            app.renaming_app = None;
                            app.edit_text.clear();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("保存").clicked() {
                                commit = Some(app.edit_text.trim().to_string());
                            }
                        });
                    });
                });
        });

    if let Some(name) = commit {
        if name.is_empty() {
            app.renaming_app = None;
        } else {
            app.commit_app_rename(&name);
            app.renaming_app = None;
            app.edit_text.clear();
        }
    }
}

// ---------------------------------------------------------------- 背景

fn paint_background(app: &mut LaunchDeckApp, ui: &egui::Ui) {
    let rect = ui.max_rect();
    let p = pal(app);

    if app.settings.display_mode == DisplayMode::Window {
        if let Some(tex) = &app.wallpaper_win {
            let sz = tex.size_vec2();
            let scale = (rect.width() / sz.x).max(rect.height() / sz.y);
            let dst = Rect::from_center_size(rect.center(), sz * scale);
            ui.painter().image(
                tex.id(),
                dst,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            return;
        }
    }
    if let Some(tex) = &app.wallpaper {
        let sz = tex.size_vec2();
        let e = ease_out(app.show_anim);
        // 打开时背景 1.04 → 1.0 轻微放大，配合淡入更柔和
        let scale = (rect.width() / sz.x).max(rect.height() / sz.y) * (1.04 - 0.04 * e);
        let dst = Rect::from_center_size(rect.center(), sz * scale);
        ui.painter().image(
            tex.id(),
            dst,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        // 轻微压暗，保证白色文字可读
        let a = if app.settings.background == BackgroundType::Frosted {
            82
        } else {
            39
        };
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_black_alpha(a));
        return;
    }

    let (top, bottom) = if p.dark {
        (Color32::from_rgb(30, 36, 48), Color32::from_rgb(9, 11, 15))
    } else {
        (Color32::from_rgb(222, 230, 242), Color32::from_rgb(248, 250, 252))
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// 搜索框 244×40，居中，中心 y=61（原版量取：上沿 41、下沿 81）
fn search_rect(rect: Rect) -> Rect {
    let w = 244.0_f32.min(rect.width() * 0.5);
    Rect::from_center_size(
        egui::pos2(rect.center().x, rect.min.y + 61.5),
        egui::vec2(w, 40.0),
    )
}

fn search_bar(app: &mut LaunchDeckApp, ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let p = pal(app);
    let r = search_rect(rect);

    // 胶囊：原版几乎是全透明的毛玻璃，只靠细描边和模糊产生层次
    ui.painter().rect_filled(
        r,
        20.0,
        Color32::from_rgba_premultiplied(5, 5, 5, 5),
    );
    let border = if ui.ctx().memory(|m| m.focused().is_some()) {
        Color32::from_rgba_premultiplied(110, 160, 245, 150)
    } else {
        Color32::from_rgba_premultiplied(33, 33, 33, 33)
    };
    ui.painter()
        .rect_stroke(r, 20.0, Stroke::new(1.0_f32, border));

    // 放大镜：原版字形 16×16，圆心约 (左沿+24, 65.5)
    let c = egui::pos2(r.min.x + 24.5, r.min.y + 23.5);
    ui.painter()
        .circle_stroke(c, 5.6, Stroke::new(1.8_f32, Color32::from_gray(195)));
    ui.painter().line_segment(
        [
            egui::pos2(c.x + 4.2, c.y + 4.2),
            egui::pos2(c.x + 8.0, c.y + 8.0),
        ],
        Stroke::new(1.8_f32, Color32::from_gray(195)),
    );

    // 空态占位文字（原版起点 x=左沿+47，中心 y=65.5）；类型筛选激活时显示筛选状态
    if app.search.is_empty() {
        let placeholder = match &app.category_filter {
            Some(c) => format!("类型：{}（Esc 清除）", LaunchDeckApp::category_label(c)),
            None => "搜索应用".to_string(),
        };
        ui.painter().text(
            egui::pos2(r.min.x + 47.0, r.min.y + 24.5),
            Align2::LEFT_CENTER,
            placeholder,
            FontId::proportional(12.5),
            Color32::from_rgb(196, 202, 208),
        );
        // 右侧圆形「⋯」按钮（原版常驻）——点击打开设置
        let bc = egui::pos2(r.max.x - 24.0, r.min.y + 24.5);
        let hovered = ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|p| (p - bc).length() < 12.0);
        let stroke = if hovered {
            Color32::from_rgba_premultiplied(215, 215, 215, 215)
        } else {
            Color32::from_rgba_premultiplied(150, 150, 150, 150)
        };
        ui.painter().circle_stroke(bc, 7.5, Stroke::new(1.2_f32, stroke));
        for dx in [-3.0_f32, 0.0, 3.0] {
            ui.painter()
                .circle_filled(egui::pos2(bc.x + dx, bc.y), 1.0, stroke);
        }
    }

    // 输入区（文字顶对齐，下移 16px 与原生基线一致）
    let text_rect = Rect::from_min_max(
        egui::pos2(r.min.x + 47.0, r.min.y + 16.0),
        egui::pos2(r.max.x - 40.0, r.max.y),
    );
    let resp = ui.put(
        text_rect,
        egui::TextEdit::singleline(&mut app.search)
            .id(egui::Id::new("search_box"))
            .text_color(Color32::from_rgb(235, 238, 241))
            .frame(false)
            .horizontal_align(egui::Align::LEFT)
            .margin(egui::vec2(0.0, 0.0)),
    );
    if app.search_focus {
        resp.request_focus();
        app.search_focus = false;
    }
    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.search.clear();
        resp.surrender_focus();
    }

    // 清除按钮
    if !app.search.is_empty() {
        let cc = egui::pos2(r.max.x - 24.0, r.min.y + 24.5);
        let hit = Rect::from_center_size(cc, egui::vec2(20.0, 20.0));
        let click = ui
            .interact(hit, egui::Id::new("search-clear"), Sense::click())
            .clicked();
        ui.painter().circle_filled(
            cc,
            8.0,
            if click {
                Color32::from_gray(210)
            } else {
                Color32::from_rgba_premultiplied(120, 120, 120, 120)
            },
        );
        ui.painter().text(
            cc,
            Align2::CENTER_CENTER,
            "✕",
            FontId::proportional(8.0),
            Color32::from_rgb(30, 32, 36),
        );
        if click {
            app.search.clear();
        }
    }
    let _ = p;
}

// ---------------------------------------------------------------- 网格

pub struct Geom {
    x0: f32,
    y0: f32,
    tile_w: f32,
    row_h: f32,
    icon: f32,
    cols: usize,
    rows: usize,
}

impl Geom {
    pub fn new(rect: Rect, cols: usize, rows: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let w = rect.width();
        let h = rect.height();
        // 窗口模式（1200×800）也走等比布局，只是整体缩小
        let full = h >= 700.0;
        // 原版稳定态量取：7 列、列距 251.6、首列中心 269，图标可见 105
        let side = if full { w * 0.070 } else { 20.0 };
        let tile_w = (w - side * 2.0) / cols as f32;
        let icon = if full {
            (tile_w * 0.515).min(131.0)
        } else {
            (tile_w * 0.50).min(84.0).max(32.0)
        };
        let top = if full { h * 0.1003 } else { 84.0 };
        let bottom_pad = if full { h * 0.1172 } else { 56.0 };
        let row_h = ((h - top - bottom_pad) / rows as f32)
            .min(if full { 180.4 } else { 140.0 })
            .max(48.0);
        Self {
            x0: side + tile_w / 2.0,
            y0: top,
            tile_w,
            row_h,
            icon,
            cols,
            rows,
        }
    }

    /// 图标本身的矩形（点击/拖拽只认这块，而不是整个格子）
    fn icon_rect(&self, cell: Rect) -> Rect {
        let off = if self.icon > 100.0 { 12.0 } else { 10.0 };
        let c = egui::pos2(cell.center().x, cell.min.y + off + self.icon / 2.0);
        Rect::from_center_size(c, Vec2::splat(self.icon))
    }

    /// 命中区：可见图标再外扩一点点，避免点到格子空白也触发
    fn hit_rect(&self, cell: Rect) -> Rect {
        self.icon_rect(cell).shrink(self.icon * 0.06)
    }

    fn cell(&self, idx: usize) -> Rect {
        let col = idx % self.cols;
        let row = idx / self.cols;
        Rect::from_min_size(
            egui::pos2(
                self.x0 + col as f32 * self.tile_w - self.tile_w / 2.0,
                self.y0 + row as f32 * self.row_h,
            ),
            egui::vec2(self.tile_w, self.row_h),
        )
    }

    fn scaled(&self, s: f32, center: egui::Pos2) -> Self {
        Self {
            x0: center.x + (self.x0 - center.x) * s,
            y0: center.y + (self.y0 - center.y) * s,
            tile_w: self.tile_w * s,
            row_h: self.row_h * s,
            icon: self.icon * s,
            cols: self.cols,
            rows: self.rows,
        }
    }

    pub fn cell_at(&self, pos: egui::Pos2) -> Option<usize> {
        let col = ((pos.x - (self.x0 - self.tile_w / 2.0)) / self.tile_w).floor() as isize;
        let row = ((pos.y - self.y0) / self.row_h).floor() as isize;
        if col < 0 || row < 0 || col >= self.cols as isize || row >= self.rows as isize {
            None
        } else {
            Some(row as usize * self.cols + col as usize)
        }
    }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn grid_page(app: &mut LaunchDeckApp, ui: &mut egui::Ui) {
    if app.apps.is_empty() {
        ui.centered_and_justified(|ui| {
            if app.scanning {
                ui.spinner();
            } else if ui.button("扫描应用").clicked() {
                app.start_scan();
            }
        });
        return;
    }

    let rect = ui.max_rect();
    let p = pal(app);
    // 打开动画：网格整体从 96.5% 放大到 100%
    let e = ease_out(app.show_anim);
    let zoom = 0.965 + 0.035 * e;
    let grid_rect = Rect::from_center_size(rect.center(), rect.size() * zoom);
    let geom = Geom::new(grid_rect, app.layout.columns, app.layout.rows);

    // 悬停光斑已按需求移除（只保留图标轻微放大）

    // 拖拽：目标图标高亮（烟灰玻璃，应用与文件夹一致——
    // 压在文件夹上即「选中文件夹」，松手直接进入；文件夹间移动同样高亮目标）
    if app.drag.is_some() {
        if let Some(pos) = ui.ctx().pointer_interact_pos() {
            if let Some(target) = geom.cell_at(pos) {
                if geom.hit_rect(geom.cell(target)).contains(pos) {
                    let hl = geom.icon_rect(geom.cell(target)).expand(2.5);
                    ui.painter().rect_filled(
                        hl,
                        hl.width() * 0.24,
                        Color32::from_rgba_premultiplied(9, 11, 15, 52),
                    );
                }
            }
            ui.ctx().request_repaint();
        }
    }

    if app.page_anim < 1.0 && app.page_prev != app.page {
        let e = ease_out(app.page_anim);
        let prev_off = -app.page_dir * rect.width() * e;
        let cur_off = app.page_dir * rect.width() * (1.0 - e);
        draw_static_page(app, ui, &p, &geom, app.page_prev, prev_off);
        draw_page(app, ui, &p, &geom, app.page, cur_off, rect, true);
        ui.ctx().request_repaint();
    } else {
        draw_page(app, ui, &p, &geom, app.page, 0.0, rect, true);
    }

    // 顶层：拖拽幽灵图标（盖在网格之上）
    if app.drag.is_some() {
        // 幽灵图标跟随指针（文件夹视图可见时它的自绘幽灵在上层，
        // 面板收起后由这里的幽灵接管，拖出的图标不会消失）
        if let (Some(pos), Some(drag)) = (
            ui.ctx().pointer_interact_pos().or_else(crate::e2e::ghost_pos),
            app.drag.as_ref(),
        ) {
            let icon = match &drag.item {
                Item::App { app: a } => app.icons.get(ui.ctx(), &a.path),
                Item::Folder { .. } => None,
            };
            if let Some(t) = icon {
                let r = Rect::from_center_size(pos, Vec2::splat(geom.icon * 0.95));
                ui.painter().image(
                    t.id(),
                    r,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::from_white_alpha(235),
                );
            }
        }
    }
}

fn page_items(app: &LaunchDeckApp, page: usize) -> Vec<Item> {
    app.layout
        .pages
        .get(page.min(app.layout.page_count().saturating_sub(1)))
        .map(|p| p.items.clone())
        .unwrap_or_default()
}

/// 翻页时只绘制、不接收交互的上一页
fn draw_static_page(
    app: &mut LaunchDeckApp,
    ui: &mut egui::Ui,
    p: &Pal,
    geom: &Geom,
    page: usize,
    offset: f32,
) {
    let items = page_items(app, page);
    for idx in 0..(geom.cols * geom.rows) {
        let Some(item) = items.get(idx) else { continue };
        let cell = geom.cell(idx).translate(egui::vec2(offset, 0.0));
        draw_tile(app, ui, p, geom, item, cell, 0.0, false);
    }
}

fn draw_page(
    app: &mut LaunchDeckApp,
    ui: &mut egui::Ui,
    p: &Pal,
    geom: &Geom,
    page: usize,
    offset: f32,
    rect: Rect,
    interactive: bool,
) {
    let items = page_items(app, page);
    let mut launch: Option<crate::layout::AppRef> = None;
    let mut open_folder: Option<String> = None;
    let mut cmd_toggle: Option<String> = None;

    let mut bg_resp: Option<egui::Response> = None;
    if interactive {
        let bg = ui.interact(rect, egui::Id::new("bg"), Sense::click_and_drag());
        bg_resp = Some(bg.clone());
        if bg.clicked() {
            if !app.selected.is_empty() {
                app.selected.clear();
            } else {
                // 原版：点击空白处退出启动台
                app.hide_window();
            }
        }
        // 窗口模式：拖空白处移动窗口（无边框窗口没有系统标题栏可拖）
        if app.settings.display_mode == DisplayMode::Window && bg.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        bg.context_menu(|ui| main_menu(app, ui));
    }

    // 拖动中的图标：从原位"拿走"，后面的图标依次前移，原位空出来；
    // 前移动画用指数平滑，图标滑过去而不是跳格
    let drag_src = app
        .drag
        .as_ref()
        .filter(|d| d.from_page == page)
        .map(|d| d.from_index);
    if app.drag.is_none() {
        app.drag_offsets.clear();
    }
    let dt = ui.ctx().input(|i| i.stable_dt).min(0.05);
    // 柔和的挤占动画：约 0.3s 缓出到位
    let smoothing = 1.0 - (-dt * 10.0).exp();

    for idx in 0..(geom.cols * geom.rows) {
        let Some(item) = items.get(idx) else { continue };
        if Some(idx) == drag_src {
            continue; // 被拖走的图标不画在原位（改用跟随指针的幽灵）
        }
        let slot = match drag_src {
            Some(src) if idx > src => idx - 1,
            _ => idx,
        };
        let mut cell = geom.cell(slot).translate(egui::vec2(offset, 0.0));
        if drag_src.is_some() {
            // 平滑：从原位滑向预览格
            let origin_x = geom.cell(idx).translate(egui::vec2(offset, 0.0)).min.x;
            let target_x = cell.min.x;
            let e = app.drag_offsets.entry((page, idx)).or_insert((origin_x, 0.0));
            e.0 += (target_x - e.0) * smoothing;
            cell = Rect::from_min_size(egui::pos2(e.0, cell.min.y), cell.size());
            ui.ctx().request_repaint();
        }
        let id = egui::Id::new(("tile", page, idx));
        let resp = ui.interact(geom.hit_rect(cell), id, Sense::click_and_drag());
        let hover_t = ui
            .ctx()
            .animate_bool_with_time(egui::Id::new(("hv", page, idx)), resp.hovered(), 0.12);

        draw_tile(
            app,
            ui,
            p,
            geom,
            item,
            cell,
            hover_t,
            app.selected.contains(item.bundle_id()),
        );

        if interactive {
            if resp.clicked() {
                let cmd = ui.input(|i| i.modifiers.command);
                match item {
                    Item::App { app: a } => {
                        if cmd {
                            cmd_toggle = Some(a.bundle_id.clone());
                        } else {
                            launch = Some(a.clone());
                        }
                    }
                    Item::Folder { folder } => open_folder = Some(folder.id.clone()),
                }
            }
            if resp.drag_started() {
                app.begin_drag(item.clone(), page, idx);
            }
            if resp.drag_stopped() {
                // 预览时源图标已被抽走、后面的格子前移了一位，
                // drop_at 现在直接接收可见格下标（内部换算原始下标）
                let src = app.drag.as_ref().map(|d| d.from_index);
                let _ = src;
                if let Some(pos) = ui.ctx().pointer_interact_pos() {
                    if let Some(slot) = geom.cell_at(pos) {
                        // 只有指针真正压在这一格的图标上才允许合并成文件夹
                        let on_icon = geom.hit_rect(geom.cell(slot)).contains(pos);
                        app.drop_at(page, slot, on_icon);
                    }
                }
                app.drag = None;
            }
            resp.context_menu(|ui| context_menu(app, ui, item, idx, page));
        }
    }

    if interactive {
        // 框选
        let cmd = ui.input(|i| i.modifiers.command);
        let pointer = ui.ctx().input(|i| i.pointer.hover_pos());
        let bg = bg_resp.as_ref();
        if cmd && bg.map(|b| b.drag_started()).unwrap_or(false) {
            if let Some(p) = pointer {
                app.marquee = Some((p, p));
            }
        }
        if let Some((start, _)) = app.marquee {
            if let Some(p) = pointer {
                app.marquee = Some((start, p));
            }
            if ui.input(|i| i.pointer.any_released()) {
                if let Some((s, e)) = app.marquee.take() {
                    let sel = Rect::from_two_pos(s, e);
                    if sel.width() > 4.0 || sel.height() > 4.0 {
                        for idx in 0..(geom.cols * geom.rows) {
                            if let Some(Item::App { app: a }) = items.get(idx) {
                                // 框选必须碰到图标本体（不是整格）才选中
                                if geom
                                    .icon_rect(geom.cell(idx))
                                    .translate(egui::vec2(offset, 0.0))
                                    .intersects(sel)
                                {
                                    app.selected.insert(a.bundle_id.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        if !cmd && bg.map(|b| b.clicked()).unwrap_or(false) {
            app.selected.clear();
        }
        if let Some((s, e)) = app.marquee {
            let r = Rect::from_two_pos(s, e);
            ui.painter().rect_filled(
                r,
                3.0,
                Color32::from_rgba_unmultiplied(90, 150, 255, 42),
            );
            ui.painter().rect_stroke(
                r,
                3.0,
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(120, 170, 255, 210)),
            );
        }

        // 拖拽落点（幽灵图标在 grid_page 顶层绘制）
        if app.drag.is_some() {
            if ui.input(|i| i.pointer.any_released()) {
                if let Some(pos) = ui.ctx().input(|i| i.pointer.hover_pos()) {
                    // 从文件夹拖出：面板内松手 = 排序；面板外压在另一个
                    // 文件夹图标上 = 直接移入该文件夹；否则移出到落点
                    if let Some(d) = app.drag.as_ref() {
                        if let Some(fid) = d.from_folder.clone() {
                            let idx = d.from_index;
                            let inside = app
                                .folder_panel_rect
                                .map(|r| r.contains(pos))
                                .unwrap_or(false);
                            app.drag = None;
                            app.folder_drag_active = false;
                            if inside {
                                if let Some(panel) = app.folder_panel_rect {
                                    app.reorder_folder(&fid, idx, pos, panel);
                                }
                                app.folder_closing = false;
                            } else {
                                // 落点压在另一个文件夹图标上 → 文件夹间移动
                                let mut dest: Option<String> = None;
                                if let Some(slot) = geom.cell_at(pos) {
                                    if geom.hit_rect(geom.cell(slot)).contains(pos) {
                                        if let Some(Item::Folder { folder }) = items.get(slot) {
                                            if folder.id != fid {
                                                dest = Some(folder.id.clone());
                                            }
                                        }
                                    }
                                }
                                if let Some(dest) = dest {
                                    app.move_app_between_folders(&fid, idx, &dest);
                                } else {
                                    app.drag_app_out_of_folder(&fid, idx, Some(pos));
                                }
                            }
                            return;
                        }
                    }
                    let target_page = if pos.x < 12.0 {
                        page.saturating_sub(1)
                    } else if pos.x > rect.max.x - 12.0 {
                        (page + 1).min(app.layout.page_count())
                    } else {
                        page
                    };
                    // 没落在任何格子里 → 追加到目标页末尾（不合并文件夹）
                    let (index, on_icon) = match geom.cell_at(pos) {
                        Some(slot) => (
                            slot,
                            geom.hit_rect(geom.cell(slot)).contains(pos),
                        ),
                        None => (items.len(), false),
                    };
                    app.drop_at(target_page, index, on_icon);
                } else {
                    app.drag = None;
                }
            }
        }

        if let Some(id) = cmd_toggle {
            if !app.selected.remove(&id) {
                app.selected.insert(id);
            }
        }
        if let Some(id) = open_folder {
            app.open_folder = Some(id);
            app.folder_closing = false;
        }
        if let Some(a) = launch {
            app.open(&a);
        }
    }
}

fn draw_tile(
    app: &mut LaunchDeckApp,
    ui: &egui::Ui,
    p: &Pal,
    geom: &Geom,
    item: &Item,
    cell: Rect,
    hover_t: f32,
    selected: bool,
) {
    let painter = ui.painter();
    let icon_top = cell.min.y + if geom.icon > 100.0 { 12.0 } else { 10.0 };
    let icon_c = egui::pos2(cell.center().x, icon_top + geom.icon / 2.0);
    let icon_size = geom.icon * (1.0 + 0.035 * hover_t);
    let icon_r = Rect::from_center_size(icon_c, Vec2::splat(icon_size));

    if selected {
        // 选中态：紧贴图标的一层小玻璃——只有深色半透底，无描边无白色元素
        let ring = icon_r.expand(2.5);
        painter.rect_filled(
            ring,
            ring.width() * 0.24,
            Color32::from_rgba_premultiplied(9, 11, 15, 56),
        );
    }


    match item {
        Item::App { app: a } => {
            // 仅在悬停时给一点柔和投影
            if hover_t > 0.01 {
                painter.rect_filled(
                    icon_r.translate(egui::vec2(0.0, 2.0)).shrink(10.0),
                    16.0,
                    Color32::from_black_alpha((32.0 * hover_t) as u8),
                );
            }
            if let Some(tex) = app.icons.get(ui.ctx(), &a.path) {
                painter.image(
                    tex.id(),
                    icon_r,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                painter.rect_filled(icon_r, 20.0, Color32::from_rgb(70, 74, 84));
                painter.text(
                    icon_c,
                    Align2::CENTER_CENTER,
                    a.name.chars().next().unwrap_or('?').to_string(),
                    FontId::proportional(geom.icon * 0.4),
                    Color32::from_gray(230),
                );
            }

            // 图标样式只影响 App 自身图标（Dock/菜单栏），不再给网格图标加描边
        }
        Item::Folder { folder } => {
            // 拖拽悬停时文件夹稍稍放大（接收指示，带 0.12s 平滑）
            let mut drag_over = false;
            if app.drag.is_some() {
                if let Some(pos) = ui.ctx().pointer_interact_pos().or_else(crate::e2e::ghost_pos) {
                    drag_over = geom.hit_rect(cell).contains(pos);
                }
            }
            let hover_t = ui.ctx().animate_bool_with_time(
                egui::Id::new(("fd-hv", cell.min.x as i32, cell.min.y as i32)),
                drag_over,
                0.12,
            );
            // 原版文件夹底是几乎透明的毛玻璃，外框尺寸与可见图标一致（约 104）
            let tile = Rect::from_center_size(
                icon_c,
                Vec2::splat(geom.icon * 0.809 * (1.0 + 0.07 * hover_t)),
            );
            let radius = tile.width() * 0.24;
            // 原版文件夹底是几乎透明的毛玻璃（约 +10 灰阶）。
            // 注意：egui 的 from_white_alpha 会把数值当线性 alpha 再转 sRGB，
            // 12 会被放大到 ~60，因此这里直接用 premultiplied 构造。
            let fill = Color32::from_rgba_premultiplied(12, 12, 12, 12);
            let line = Color32::from_rgba_premultiplied(45, 45, 45, 45);
            painter.rect_filled(tile, radius, fill);
            painter.rect_stroke(tile, radius, Stroke::new(1.0_f32, line));
            // 预览 3×3（原版只显示前 9 个），行距 27.3
            let n = folder.apps.len().min(9);
            let icon_s = tile.width() * 0.243;
            let pitch = icon_s + tile.width() * 0.019;
            let grid_w = 2.0 * pitch + icon_s;
            let start = egui::pos2(
                tile.center().x - grid_w / 2.0,
                tile.center().y - grid_w / 2.0,
            );
            for i in 0..n {
                let cr = Rect::from_min_size(
                    egui::pos2(
                        start.x + (i % 3) as f32 * pitch,
                        start.y + (i / 3) as f32 * pitch,
                    ),
                    Vec2::splat(icon_s),
                );
                if let Some(t) = app.icons.get(ui.ctx(), &folder.apps[i].path) {
                    painter.image(
                        t.id(),
                        cr,
                        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    painter.rect_filled(cr, 4.0, Color32::from_rgb(110, 114, 124));
                }
            }
        }
    }



    if !app.layout.hide_names {
        // 原版不截断长名称（如「卸载 Adobe Photoshop 2026」会溢出单元格）
        let label = truncate(item.display_name(), if geom.icon < 70.0 { 10 } else { 40 });
        // 原版：标签核心中心距图标底边 12；SF Pro 在原生渲染下带约 1.25px 字距
        let pos = egui::pos2(
            cell.center().x,
            icon_top + geom.icon + if geom.icon > 100.0 { 12.0 } else { 18.0 },
        );
        let font = FontId::proportional(12.5);
        // 文字投影
        paint_tracked(
            ui,
            pos + egui::vec2(0.0, 1.0),
            &label,
            &font,
            Color32::from_black_alpha(120),
            1.25,
        );
        paint_tracked(
            ui,
            pos,
            &label,
            &font,
            if p.dark {
                Color32::from_rgb(245, 245, 248)
            } else {
                Color32::from_rgb(30, 32, 38)
            },
            1.25,
        );
    }
}

/// 按原版 SF Pro 的字距绘制居中文案（egui 不原生支持 letter-spacing）
fn paint_tracked(
    ui: &egui::Ui,
    center: egui::Pos2,
    text: &str,
    font: &FontId,
    color: Color32,
    tracking: f32,
) {
    let widths: Vec<f32> = text
        .chars()
        .map(|c| ui.fonts(|f| f.glyph_width(font, c)))
        .collect();
    if widths.is_empty() {
        return;
    }
    let total = widths.iter().sum::<f32>() + tracking * widths.len().saturating_sub(1) as f32;
    let mut x = center.x - total / 2.0;
    for (ch, w) in text.chars().zip(widths) {
        ui.painter().text(
            egui::pos2(x + w / 2.0, center.y),
            Align2::CENTER_CENTER,
            ch,
            font.clone(),
            color,
        );
        x += w + tracking;
    }
}

fn page_dots(app: &mut LaunchDeckApp, ui: &mut egui::Ui) {
    if app.search.trim().is_empty() == false {
        return;
    }
    let n = app.layout.page_count();
    if n <= 1 {
        return;
    }
    let rect = ui.max_rect();
    // 原版页点中心 y=1043.5，间距 20，实心点直径 8
    let y = rect.max.y - 108.5;
    let spacing = 20.0;
    let start = rect.center().x - (n as f32 - 1.0) * spacing / 2.0;
    for i in 0..n {
        let c = egui::pos2(start + i as f32 * spacing, y);
        let active = i == app.page;
        let resp = ui.interact(
            Rect::from_center_size(c, egui::vec2(18.0, 18.0)),
            egui::Id::new(("dot", i)),
            Sense::click(),
        );
        ui.painter().circle_filled(
            c,
            if active { 4.0 } else { 3.5 },
            if active {
                // 原版实心点 ≈ 88% 白，未选中点 ≈ 30% 白
                Color32::from_rgba_premultiplied(221, 221, 221, 221)
            } else {
                Color32::from_rgba_premultiplied(78, 78, 78, 78)
            },
        );
        if resp.clicked() {
            app.set_page(i, ui.ctx());
        }
    }
}

fn search_results(app: &mut LaunchDeckApp, ui: &mut egui::Ui) {
    let results = app.search_results();
    results_grid(app, ui, results, "没有找到匹配的应用", true);
}

/// 类型筛选视图（类型筛选激活且搜索为空时显示）
fn category_results(app: &mut LaunchDeckApp, ui: &mut egui::Ui) {
    let results = app.filtered_items();
    results_grid(app, ui, results, "该类型下没有应用", false);
}

/// 搜索 / 类型筛选共用的结果网格；`clear_search` 决定点空白清除搜索还是清除筛选
fn results_grid(
    app: &mut LaunchDeckApp,
    ui: &mut egui::Ui,
    results: Vec<Item>,
    empty: &str,
    clear_search: bool,
) {
    let rect = ui.max_rect();
    let p = pal(app);
    if results.is_empty() {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            empty,
            FontId::proportional(13.0),
            Color32::from_white_alpha(140),
        );
        return;
    }
    let geom = Geom::new(rect, app.layout.columns, app.layout.rows);
    let mut launch: Option<crate::layout::AppRef> = None;
    let mut open_folder: Option<String> = None;
    let mut clear = false;

    let bg = ui.interact(rect, egui::Id::new("results-bg"), Sense::click());
    if bg.clicked() {
        if let Some(pos) = ui.ctx().input(|i| i.pointer.hover_pos()) {
            if geom.cell_at(pos).is_none() {
                clear = true;
            }
        }
    }

    for (idx, item) in results.iter().enumerate().take(geom.cols * geom.rows) {
        let cell = geom.cell(idx);
        let resp = ui.interact(geom.hit_rect(cell), egui::Id::new(("s-tile", idx)), Sense::click());
        let t = ui
            .ctx()
            .animate_bool_with_time(egui::Id::new(("s-hv", idx)), resp.hovered(), 0.12);
        draw_tile(app, ui, &p, &geom, item, cell, t, false);
        if resp.clicked() {
            match item {
                Item::App { app: a } => launch = Some(a.clone()),
                Item::Folder { folder } => open_folder = Some(folder.id.clone()),
            }
        }
        resp.context_menu(|ui| context_menu(app, ui, item, idx, app.page));
    }

    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        if let Some(Item::App { app: a }) = results.first() {
            launch = Some(a.clone());
        }
    }
    if clear {
        if clear_search {
            app.search.clear();
        } else {
            app.category_filter = None;
        }
    }
    if let Some(id) = open_folder {
        app.open_folder = Some(id);
    }
    if let Some(a) = launch {
        app.open(&a);
        if clear_search {
            app.search.clear();
        }
    }
}

// ---------------------------------------------------------------- 滚动浏览

/// 连续滚动模式：所有应用按列排成一张长网格，滚轮纵向浏览
fn scroll_view(app: &mut LaunchDeckApp, ui: &mut egui::Ui) {
    if app.apps.is_empty() {
        ui.centered_and_justified(|ui| {
            if app.scanning {
                ui.spinner();
            } else if ui.button("扫描应用").clicked() {
                app.start_scan();
            }
        });
        return;
    }

    let rect = ui.max_rect();
    let p = pal(app);
    let e = ease_out(app.show_anim);
    let zoom = 0.965 + 0.035 * e;
    let grid_rect = Rect::from_center_size(rect.center(), rect.size() * zoom);
    let cols = app.layout.columns.max(1);
    let geom = Geom::new(grid_rect, cols, app.layout.rows);
    let total: Vec<Item> = app.layout.pages.iter().flat_map(|pg| pg.items.clone()).collect();
    let rows_n = total.len().div_ceil(cols);
    let content_h = geom.y0 + rows_n as f32 * geom.row_h + 40.0;
    let max_scroll = (content_h - rect.height()).max(0.0);
    app.browse_scroll = app.browse_scroll.clamp(0.0, max_scroll);

    // 滚轮
    let dy = ui.input(|i| i.raw_scroll_delta.y);
    if dy != 0.0 {
        app.browse_scroll = (app.browse_scroll - dy).clamp(0.0, max_scroll);
        ui.ctx().request_repaint();
    }

    // 点空白处收起（与分页模式一致）
    let bg = ui.interact(rect, egui::Id::new("scroll-bg"), Sense::click());
    if bg.clicked() {
        if !app.selected.is_empty() {
            app.selected.clear();
        } else {
            app.hide_window();
        }
    }

    let off = app.browse_scroll;
    let cap = app.layout.capacity().max(1);
    let drag_src = app.drag.as_ref().map(|d| d.from_page * cap + d.from_index);
    if app.drag.is_none() {
        app.drag_offsets.clear();
    }
    let dt = ui.ctx().input(|i| i.stable_dt).min(0.05);
    let smoothing = 1.0 - (-dt * 10.0).exp();

    // 命中计算要把滚动偏移加回去
    let slot_at = |pos: egui::Pos2| -> Option<usize> {
        let col = ((pos.x - (geom.x0 - geom.tile_w / 2.0)) / geom.tile_w).floor() as isize;
        let row = ((pos.y + off - geom.y0) / geom.row_h).floor() as isize;
        if col < 0 || row < 0 || col >= cols as isize {
            None
        } else {
            Some(row as usize * cols + col as usize)
        }
    };

    let mut launch: Option<crate::layout::AppRef> = None;
    let mut open_folder: Option<String> = None;
    let mut cmd_toggle: Option<String> = None;

    for (flat, item) in total.iter().enumerate() {
        if drag_src == Some(flat) {
            continue; // 被拖走的图标不画在原位
        }
        let slot = match drag_src {
            Some(s) if flat > s => flat - 1,
            _ => flat,
        };
        let mut cell = geom.cell(slot).translate(egui::vec2(0.0, -off));
        if drag_src.is_some() {
            // 与分页视图一致的平滑：图标滑向预览格
            let origin_x = geom.cell(flat).min.x;
            let target_x = geom.cell(slot).min.x;
            let e = app
                .drag_offsets
                .entry((usize::MAX, flat))
                .or_insert((origin_x, 0.0));
            e.0 += (target_x - e.0) * smoothing;
            cell = Rect::from_min_size(
                egui::pos2(e.0, cell.min.y),
                cell.size(),
            );
            ui.ctx().request_repaint();
        }
        if cell.max.y < rect.min.y - 40.0 || cell.min.y > rect.max.y + 40.0 {
            continue; // 视口外不画不交互
        }
        let resp = ui.interact(geom.hit_rect(cell), egui::Id::new(("s-tile", flat)), Sense::click_and_drag());
        let hover_t = ui
            .ctx()
            .animate_bool_with_time(egui::Id::new(("s-hv", flat)), resp.hovered(), 0.12);
        draw_tile(
            app,
            ui,
            &p,
            &geom,
            item,
            cell,
            hover_t,
            app.selected.contains(item.bundle_id()),
        );
        if resp.clicked() {
            let cmd = ui.input(|i| i.modifiers.command);
            match item {
                Item::App { app: a } => {
                    if cmd {
                        cmd_toggle = Some(a.bundle_id.clone());
                    } else {
                        launch = Some(a.clone());
                    }
                }
                Item::Folder { folder } => open_folder = Some(folder.id.clone()),
            }
        }
        if resp.drag_started() {
            let (pg, idx) = (flat / cap, flat % cap);
            app.begin_drag(item.clone(), pg, idx);
        }
        resp.context_menu(|ui| context_menu(app, ui, item, flat % cap, flat / cap));
    }

    // 拖拽预览 + 落点（高亮只在压到图标上显示，烟灰玻璃；压在文件夹上同样
    // 高亮，松手直接进入，不展开预览）
    if app.drag.is_some() {
        if let Some(pos) = ui.ctx().pointer_interact_pos().or_else(crate::e2e::ghost_pos) {
            if let Some(target) = slot_at(pos) {
                let cell = geom.cell(target);
                if geom.hit_rect(cell).contains(pos) {
                    let hl = geom
                        .icon_rect(cell.translate(egui::vec2(0.0, -off)))
                        .expand(2.5);
                    ui.painter().rect_filled(
                        hl,
                        hl.width() * 0.24,
                        Color32::from_rgba_premultiplied(9, 11, 15, 52),
                    );
                }
            }
        }

        if let Some(pos) = ui.ctx().pointer_interact_pos() {
            let icon = match &app.drag.as_ref().unwrap().item {
                Item::App { app: a } => app.icons.get(ui.ctx(), &a.path),
                Item::Folder { .. } => None,
            };
            if let Some(t) = icon {
                let r = Rect::from_center_size(pos, Vec2::splat(geom.icon * 0.95));
                ui.painter().image(
                    t.id(),
                    r,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::from_white_alpha(235),
                );
            }
            ui.ctx().request_repaint();
        }
        if ui.input(|i| i.pointer.any_released()) {
            if let Some(pos) = ui.ctx().input(|i| i.pointer.hover_pos()) {
                // 从文件夹拖出：面板内 = 排序；面板外 = 移出（落点按分页网格近似）
                if let Some(d) = app.drag.as_ref() {
                    if let Some(fid) = d.from_folder.clone() {
                        let idx = d.from_index;
                        let inside = app
                            .folder_panel_rect
                            .map(|r| r.contains(pos))
                            .unwrap_or(false);
                        app.drag = None;
                        app.folder_drag_active = false;
                        if inside {
                            if let Some(panel) = app.folder_panel_rect {
                                app.reorder_folder(&fid, idx, pos, panel);
                            }
                            app.folder_closing = false;
                        } else {
                            app.drag_app_out_of_folder(&fid, idx, Some(pos));
                        }
                        return;
                    }
                }
                // 没落在格子里 → 追加到末尾；压在图标上才允许合并
                let (slot, on_icon) = match slot_at(pos) {
                    Some(s) => (
                        s,
                        geom.hit_rect(geom.cell(s).translate(egui::vec2(0.0, -off)))
                            .contains(pos),
                    ),
                    None => (total.len(), false),
                };
                let (pg, idx) = (slot / cap, slot % cap);
                app.drop_at(pg, idx, on_icon);
            }
            app.drag = None;
        }
    }

    if let Some(id) = cmd_toggle {
        if !app.selected.remove(&id) {
            app.selected.insert(id);
        }
    }
    if let Some(f) = open_folder {
        app.open_folder = Some(f);
        app.folder_closing = false;
    }
    if let Some(a) = launch {
        app.open(&a);
    }

    // 滚动条
    if max_scroll > 0.0 {
        let track = Rect::from_min_size(
            egui::pos2(rect.max.x - 6.0, rect.min.y + 4.0),
            egui::vec2(3.0, rect.height() - 8.0),
        );
        let bar_h = (track.height() * rect.height() / content_h).max(24.0);
        let bar_y = track.min.y + (track.height() - bar_h) * (off / max_scroll);
        let bar = Rect::from_min_size(
            egui::pos2(track.min.x, bar_y),
            egui::vec2(track.width(), bar_h),
        );
        ui.painter()
            .rect_filled(bar, 1.5, Color32::from_rgba_premultiplied(200, 200, 200, 90));
        let resp = ui.interact(bar.expand(4.0), egui::Id::new("scroll-thumb"), Sense::drag());
        if resp.dragged() {
            let delta = ui.ctx().input(|i| i.pointer.delta().y);
            let ratio = max_scroll / (track.height() - bar_h).max(1.0);
            app.browse_scroll = (app.browse_scroll + delta * ratio).clamp(0.0, max_scroll);
            ui.ctx().request_repaint();
        }
    }
}

// ---------------------------------------------------------------- 主菜单

/// 主菜单内容（空白处右键与搜索栏「⋯」共用）
/// 弹出菜单行：悬停高亮 + 左对齐文字（替代 egui 默认按钮的原生菜单观感）
fn menu_row(ui: &mut egui::Ui, text: &str, checked: bool) -> bool {
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 27.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(
            r,
            6.0,
            Color32::from_rgba_premultiplied(255, 255, 255, 22),
        );
    }
    let mut x = r.min.x + 12.0;
    if checked {
        ui.painter().text(
            egui::pos2(x + 5.0, r.center().y),
            Align2::LEFT_CENTER,
            "✓",
            FontId::proportional(11.0),
            Color32::from_rgb(120, 170, 255),
        );
        x += 20.0;
    }
    ui.painter().text(
        egui::pos2(x, r.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(12.5),
        Color32::from_rgb(236, 239, 243),
    );
    resp.clicked()
}

/// 弹出菜单分隔线
fn menu_sep(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 9.0), egui::Sense::hover());
    ui.painter().line_segment(
        [
            egui::pos2(r.min.x + 9.0, r.center().y),
            egui::pos2(r.max.x - 9.0, r.center().y),
        ],
        Stroke::new(
            1.0_f32,
            Color32::from_rgba_premultiplied(255, 255, 255, 18),
        ),
    );
}

fn main_menu(app: &mut LaunchDeckApp, ui: &mut egui::Ui) {
    ui.set_width(184.0);
    if menu_row(ui, "设置…", false) {
        app.tab = Tab::Settings;
        app.main_menu_open = false;
        ui.close_menu();
    }
    if menu_row(ui, "全选", false) {
        app.select_all_page();
        app.main_menu_open = false;
        ui.close_menu();
    }
    if !app.selected.is_empty() && menu_row(ui, "取消选择", false) {
        app.selected.clear();
        app.main_menu_open = false;
        ui.close_menu();
    }
    menu_sep(ui);
    if menu_row(ui, if app.main_menu_types_open { "类型 ▾" } else { "类型 ▸" }, app.category_filter.is_some()) {
        app.main_menu_types_open = !app.main_menu_types_open;
    }
    if app.main_menu_types_open {
        // 类别手风琴：与行同样的交互，选中项打勾
        let mut all = app.category_filter.is_none();
        if menu_row(ui, "全部", all) && all {
            app.set_category_filter(None);
            app.main_menu_open = false;
            ui.close_menu();
        }
        let opts = app.category_options();
        for (raw, label) in opts {
            let on = app.category_filter.as_deref() == Some(raw.as_str());
            if menu_row(ui, label, on) && !on {
                app.set_category_filter(Some(raw));
                app.main_menu_open = false;
                ui.close_menu();
            }
        }
    }
    menu_sep(ui);
    if menu_row(ui, "刷新应用", false) {
        app.start_scan();
        app.main_menu_open = false;
        ui.close_menu();
    }
    if menu_row(ui, "卸载应用…", false) {
        app.tab = Tab::Uninstall;
        app.main_menu_open = false;
        ui.close_menu();
    }
    menu_sep(ui);
    if menu_row(ui, "退出", false) {
        std::process::exit(0);
    }
}

/// 搜索栏「⋯」按钮弹出的主菜单浮层
fn draw_main_menu(app: &mut LaunchDeckApp, ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    let r = search_rect(screen);
    let anchor = egui::pos2((r.max.x - 150.0).max(screen.min.x + 8.0), r.max.y + 8.0);
    let mut area_rect = Rect::NOTHING;
    egui::Area::new(egui::Id::new("main-menu-popup"))
        .fixed_pos(anchor)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(Color32::from_rgba_premultiplied(26, 28, 34, 242))
                .rounding(12.0)
                .stroke(Stroke::new(
                    1.0_f32,
                    Color32::from_rgba_premultiplied(255, 255, 255, 26),
                ))
                // 阴影让菜单浮在网格上
                .shadow(egui::epaint::Shadow {
                    offset: egui::vec2(0.0, 6.0),
                    blur: 24.0,
                    spread: 0.0,
                    color: Color32::from_black_alpha(90),
                })
                .inner_margin(egui::Margin::symmetric(4.0, 5.0))
                .show(ui, |ui| {
                    main_menu(app, ui);
                });
            area_rect = ui.min_rect();
        });
    // 打开当帧的点击就是「⋯」本身，跳过；之后点击菜单外任意位置关闭
    if ctx.frame_nr() > app.menu_open_at_frame + 1 && ctx.input(|i| i.pointer.any_click()) {
        let inside = ctx
            .input(|i| i.pointer.latest_pos())
            .map(|pos| area_rect.expand(6.0).contains(pos))
            .unwrap_or(false);
        if !inside {
            app.main_menu_open = false;
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

// ---------------------------------------------------------------- 菜单

fn context_menu(app: &mut LaunchDeckApp, ui: &mut egui::Ui, item: &Item, index: usize, page: usize) {
    ui.set_min_width(150.0);
    if app.selected.len() > 1 && app.selected.contains(item.bundle_id()) {
        ui.label(
            egui::RichText::new(format!("已选择 {} 个应用", app.selected.len()))
                .size(11.0)
                .color(Color32::from_gray(150)),
        );
        ui.separator();
        if ui.button("用所选应用新建文件夹").clicked() {
            app.folder_from_selection();
            ui.close_menu();
        }
        if ui.button("隐藏所选").clicked() {
            app.hide_selected();
            ui.close_menu();
        }
        if ui.button("移入废纸篓…").clicked() {
            app.confirm_batch = true;
            ui.close_menu();
        }
        if ui.button("取消选择").clicked() {
            app.selected.clear();
            ui.close_menu();
        }
        return;
    }
    match item {
        Item::App { app: a } => {
            if ui.button("打开").clicked() {
                app.open(a);
                ui.close_menu();
            }
            if ui.button("在访达中显示").clicked() {
                app.reveal(a);
                ui.close_menu();
            }
            if ui.button("添加到程序坞").clicked() {
                app.add_to_dock(a);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("隐藏").clicked() {
                app.toggle_hide(a);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("显示简介").clicked() {
                app.show_info(a);
                ui.close_menu();
            }
            if ui.button("重命名").clicked() {
                app.editing_folder = None;
                app.renaming_app = if a.bundle_id.is_empty() {
                    Some(a.path.clone())
                } else {
                    Some(a.bundle_id.clone())
                };
                app.edit_text = a.name.clone();
                app.focus_editing = true;
                ui.close_menu();
            }
            if ui.button("卸载").clicked() {
                let bid = if a.bundle_id.is_empty() {
                    a.path.clone()
                } else {
                    a.bundle_id.clone()
                };
                app.open_uninstall_detail(&bid);
                ui.close_menu();
            }
        }
        Item::Folder { folder } => {
            if ui.button("打开").clicked() {
                app.open_folder = Some(folder.id.clone());
                ui.close_menu();
            }
            if ui.button("重命名").clicked() {
                app.editing_folder = Some(folder.id.clone());
                app.edit_text = folder.name.clone();
                app.focus_editing = true;
                app.open_folder = Some(folder.id.clone());
                ui.close_menu();
            }
            if ui.button("取消分组").clicked() {
                app.ungroup(&folder.id);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("新建文件夹").clicked() {
                app.create_folder(page, index, None);
                ui.close_menu();
            }
        }
    }
}

// ---------------------------------------------------------------- 文件夹

fn folder_view(app: &mut LaunchDeckApp, ctx: &egui::Context) {
    let Some(fid) = app.open_folder.clone() else { return };
    let Some(folder) = app
        .layout
        .pages
        .iter()
        .flat_map(|p| p.items.iter())
        .find_map(|it| match it {
            Item::Folder { folder } if folder.id == fid => Some(folder.clone()),
            _ => None,
        })
    else {
        app.open_folder = None;
        return;
    };

    // 时间驱动的开合动画（缓出），比 animate_bool 更可控也更顺滑
    let dt = ctx.input(|i| i.stable_dt).min(0.05);
    let target = if app.folder_closing { 0.0 } else { 1.0 };
    let dur = if target > 0.5 { 0.26 } else { 0.18 };
    let step = dt / dur;
    if app.folder_anim < target {
        app.folder_anim = (app.folder_anim + step).min(target);
    } else if app.folder_anim > target {
        app.folder_anim = (app.folder_anim - step).max(target);
    }
    if app.folder_closing && app.folder_anim <= 0.001 {
        if app.folder_drag_active {
            // 拖拽中面板已完全收起：停止绘制（网格接管落点，全局 release 路由）。
            // 面板矩形必须清掉，否则残留的中央大矩形会把随后的松手误判成「放回」
            app.folder_panel_rect = None;
            return;
        }
        app.open_folder = None;
        app.folder_closing = false;
        app.folder_anim = 0.0;
        app.folder_panel_rect = None;
        return;
    }
    if (app.folder_anim - target).abs() > 0.001 {
        ctx.request_repaint();
    }
    let anim = ease_out(app.folder_anim);

    let screen = ctx.screen_rect();
    let p = pal(app);
    // 面板宽度按原版量取；高度按行数收缩（应用少时只有一行高）
    let left = screen.width() * 0.0703;
    let right = screen.width() * 0.0703;
    let max_h = screen.height() * 0.7856;
    let center_y = screen.min.y + screen.height() * 0.4944;
    let cols_est = app.layout.columns.max(1);
    let rows_needed_est = folder.apps.len().div_ceil(cols_est).max(1);
    let row_h_est = 178.7;
    let wanted_h = 12.0 + rows_needed_est.min(5) as f32 * row_h_est;
    let panel0 = Rect::from_center_size(
        egui::pos2(screen.center().x, center_y),
        egui::vec2(screen.width() - left - right, wanted_h.min(max_h)),
    );
    // 展开动画：从 86% 放大到 100%
    let scale = 0.86 + 0.14 * anim;
    let panel = Rect::from_center_size(panel0.center(), panel0.size() * scale);
    // 记录面板区域（拖出文件夹时的"放回 = 取消"判定用）
    app.folder_panel_rect = Some(panel);

    // 文件夹背板：一层更重的模糊壁纸，把下面的图标糊掉（原版打开文件夹时的观感）
    let dim = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("folder-dim"),
    ));
    if let Some(tex) = &app.wallpaper {
        let a = (255.0 * anim) as u8;
        let sz = tex.size_vec2();
        let scale = (screen.width() / sz.x).max(screen.height() / sz.y);
        let dst = Rect::from_center_size(screen.center(), sz * scale);
        dim.image(
            tex.id(),
            dst,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::from_rgba_premultiplied(a, a, a, a),
        );
        // 与启动台背景使用同一档压暗（跟随「背景类型」设置）
        let base = if app.settings.background == BackgroundType::Frosted {
            82.0
        } else {
            39.0
        };
        let dim_a = (base * anim) as u8;
        dim.rect_filled(screen, 0.0, Color32::from_black_alpha(dim_a));
    }

    let mut launch: Option<crate::layout::AppRef> = None;
    let mut close_request = false;

    egui::Area::new(egui::Id::new("folder"))
        .fixed_pos(screen.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_opacity(anim);
            // 点击面板外部收起（先注册，图标在其上层优先接收点击）
            let bg = ui.interact(
                screen,
                egui::Id::new("folder-close-bg"),
                Sense::click(),
            );

            let painter = ui.painter();
            painter.rect_filled(
                panel,
                24.0 * scale,
                if p.dark {
                    // 原版面板仅比背景亮约 6 个灰阶（毛玻璃几乎透明）
                    Color32::from_rgba_premultiplied(6, 6, 6, 6)
                } else {
                    Color32::from_rgba_premultiplied(240, 240, 242, 210)
                },
            );
            // 描边：左右很淡，上下各有一条高光
            painter.rect_stroke(
                panel,
                24.0 * scale,
                Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(8, 8, 8, 8)),
            );
            let hi = Color32::from_rgba_premultiplied(55, 55, 55, 55);
            for y in [panel.min.y, panel.max.y] {
                painter.line_segment(
                    [
                        egui::pos2(panel.min.x + 28.0 * scale, y),
                        egui::pos2(panel.max.x - 28.0 * scale, y),
                    ],
                    Stroke::new(1.0_f32, hi),
                );
            }
            // 标题（原版：重命名时就地变成输入框）
            let title_c = egui::pos2(screen.center().x, panel.min.y - 45.0 * scale);
            if app.editing_folder.as_deref() == Some(&folder.id) {
                let tr = Rect::from_center_size(title_c, egui::vec2(260.0, 30.0));
                painter.rect_filled(
                    tr,
                    8.0,
                    Color32::from_rgba_premultiplied(20, 20, 20, 90),
                );
                let resp = ui.put(
                    tr.shrink2(egui::vec2(8.0, 2.0)),
                    egui::TextEdit::singleline(&mut app.edit_text)
                        .id(egui::Id::new("folder-rename"))
                        .font(FontId::proportional(20.0))
                        .horizontal_align(egui::Align::Center)
                        .frame(false),
                );
                if app.focus_editing {
                    resp.request_focus();
                    app.focus_editing = false;
                }
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if enter || resp.lost_focus() {
                    let name = if app.edit_text.trim().is_empty() {
                        "文件夹".to_string()
                    } else {
                        app.edit_text.trim().to_string()
                    };
                    app.rename_folder(&folder.id.clone(), &name);
                    app.editing_folder = None;
                } else if esc {
                    app.editing_folder = None;
                    app.edit_text = folder.name.clone();
                }
            } else {
                painter.text(
                    title_c,
                    Align2::CENTER_CENTER,
                    &folder.name,
                    FontId::proportional(24.0 * scale),
                    Color32::from_rgb(245, 245, 248),
                );
                // 双击标题就地重命名
                let tr = Rect::from_center_size(title_c, egui::vec2(280.0, 32.0));
                let tresp = ui.interact(tr, egui::Id::new("folder-title"), Sense::click());
                if tresp.double_clicked() {
                    app.editing_folder = Some(folder.id.clone());
                    app.edit_text = folder.name.clone();
                    app.focus_editing = true;
                }
            }

            // 根据应用数量自适应行高，超出时滚动，保证不越出面板
            let cols = app.layout.columns.max(1);
            let rows_needed = folder.apps.len().div_ceil(cols).max(1);
            let avail_h = panel.height() - 12.0 * scale;
            let mut row_h = (avail_h / rows_needed as f32).min(178.7 * scale);
            let scrollable = row_h < 92.0 * scale;
            if scrollable {
                row_h = 92.0 * scale;
            }
            let visible_rows = ((avail_h / row_h).floor() as usize)
                .max(1)
                .min(rows_needed);
            let max_scroll = rows_needed.saturating_sub(visible_rows) as f32;
            if max_scroll > 0.0 {
                let wheel = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                if wheel.abs() > 0.1 {
                    app.folder_scroll =
                        (app.folder_scroll - wheel / row_h).clamp(0.0, max_scroll);
                }
                app.folder_scroll = app.folder_scroll.clamp(0.0, max_scroll);
            } else {
                app.folder_scroll = 0.0;
            }

            let base = Geom::new(screen, cols, app.layout.rows);
            let icon = (row_h - 34.0 * scale).min(base.icon * scale).max(28.0);
            // 面板内网格按面板宽度均分（原版首列中心 ≈ 270）
            let ftile = panel.width() / cols as f32;
            let geom = Geom {
                x0: panel.min.x + ftile / 2.0,
                y0: panel.min.y + 6.0 * scale - app.folder_scroll * row_h,
                tile_w: ftile,
                row_h,
                icon,
                cols,
                rows: visible_rows,
            };

            ui.set_clip_rect(panel);
            let first_row = app.folder_scroll.floor() as usize;
            // 拖出 / 排序预览：被拖图标从原位移除，插入到指针所在的预览槽位
            //（拖向面板外时保持「少一个图标」；在面板内移动时实时换位）
            let dragging_this = app
                .drag
                .as_ref()
                .map(|d| d.from_folder.as_deref() == Some(folder.id.as_str()))
                .unwrap_or(false);
            let from_idx = if dragging_this {
                app.drag.as_ref().map(|d| d.from_index)
            } else {
                None
            };
            // 拖出收起：被拖图标一离开面板边缘就收起（粘性），随后即可在
            // 网格上自由拖放、放进其他文件夹。触发必须独立于瓦片交互——
            // 被拖瓦片已跳过渲染，挂在其响应上的检查永远不会触发
            if dragging_this && !app.folder_closing {
                let ppos = ui
                    .ctx()
                    .pointer_interact_pos()
                    .or_else(crate::e2e::ghost_pos);
                if let Some(pos) = ppos {
                    let margin = geom.icon * 0.55;
                    if !panel.shrink(margin).contains(pos) {
                        app.folder_closing = true;
                        ctx.request_repaint();
                    }
                }
            }
            let dt2 = ctx.input(|i| i.stable_dt).min(0.05);
            let folder_smoothing = 1.0 - (-dt2 * 10.0).exp();
            if !dragging_this {
                app.drag_offsets.clear();
            }
            let mut order: Vec<usize> = (0..folder.apps.len()).collect();
            if let Some(fi) = from_idx {
                if fi < order.len() {
                    order.remove(fi);
                }
                if let Some(pos) = ui
                    .ctx()
                    .pointer_interact_pos()
                    .or_else(crate::e2e::ghost_pos)
                {
                    if panel.contains(pos) {
                        let col = ((pos.x - panel.min.x) / ftile).floor().max(0.0) as usize;
                        let row = ((pos.y - geom.y0) / row_h).floor().max(0.0) as usize;
                        let ps = (row * cols + col).min(order.len());
                        order.insert(ps, fi);
                    }
                }
            }
            for (disp, &ai) in order.iter().enumerate() {
                if Some(ai) == from_idx {
                    continue; // 被拖图标原位空出（幽灵跟随指针）
                }
                let a = &folder.apps[ai];
                let row = disp / cols;
                if row < first_row || row >= first_row + visible_rows {
                    continue;
                }
                let mut cell = geom
                    .cell(disp - first_row * cols)
                    .translate(egui::vec2(0.0, 0.0));
                if from_idx.is_some() {
                    // 平滑：从原位滑向预览槽位（二维）
                    let origin = geom.cell(ai - first_row * cols);
                    let e = app
                        .drag_offsets
                        .entry((usize::MAX - 1, ai))
                        .or_insert((origin.min.x, origin.min.y));
                    e.0 += (cell.min.x - e.0) * folder_smoothing;
                    e.1 += (cell.min.y - e.1) * folder_smoothing;
                    cell = Rect::from_min_size(egui::pos2(e.0, e.1), cell.size());
                    ctx.request_repaint();
                }
                let resp = ui.interact(
                    geom.hit_rect(cell),
                    egui::Id::new(("f-tile", ai)),
                    Sense::click_and_drag(),
                );
                let t = ui.ctx().animate_bool_with_time(
                    egui::Id::new(("f-hv", ai)),
                    resp.hovered(),
                    0.12,
                );
                draw_tile(app, ui, &p, &geom, &Item::app(a.clone()), cell, t, false);
                if resp.clicked() {
                    launch = Some(a.clone());
                }
                // 拖出文件夹：登记全局拖拽（带 from_folder 标记），
                // 落点由全局 release 路由（面板收起后本视图不再接收事件）
                if resp.drag_started() {
                    app.folder_drag_active = true;
                    app.drag = Some(crate::app::DragState {
                        item: Item::app(a.clone()),
                        from_page: usize::MAX,
                        from_index: ai,
                        from_folder: Some(folder.id.clone()),
                    });
                }
            }

            // 拖出中的幽灵：从拖拽状态独立绘制（不依赖瓦片交互——
            // 被拖瓦片已跳过渲染，挂在其交互上的幽灵永远不会触发）。
            // 先恢复整屏裁剪：面板内容裁剪会让面板外的幽灵被裁掉（看不见）
            ui.set_clip_rect(screen);
            if let (Some(pos), Some(d)) = (
                ui.ctx().pointer_interact_pos().or_else(crate::e2e::ghost_pos),
                app.drag.as_ref(),
            ) {
                if d.from_folder.as_deref() == Some(folder.id.as_str()) {
                    if let Item::App { app: a } = &d.item {
                        if let Some(t) = app.icons.get(ui.ctx(), &a.path) {
                            let r = Rect::from_center_size(pos, egui::vec2(geom.icon, geom.icon));
                            ui.painter().image(
                                t.id(),
                                r,
                                Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(1.0, 1.0),
                                ),
                                Color32::WHITE,
                            );
                        }
                    }
                }
            }

            // 滚动条
            if max_scroll > 0.0 {
                let track = Rect::from_min_size(
                    egui::pos2(panel.max.x - 10.0, panel.min.y + 12.0),
                    egui::vec2(3.0, panel.height() - 24.0),
                );
                let frac = visible_rows as f32 / rows_needed as f32;
                let thumb_h = (track.height() * frac).max(24.0);
                let t = if max_scroll > 0.0 {
                    app.folder_scroll / max_scroll
                } else {
                    0.0
                };
                let thumb = Rect::from_min_size(
                    egui::pos2(track.min.x, track.min.y + (track.height() - thumb_h) * t),
                    egui::vec2(3.0, thumb_h),
                );
                ui.painter()
                    .rect_filled(thumb, 2.0, Color32::from_white_alpha(60));
            }

            // 点击任何非图标位置都收起文件夹（图标点击会被 tile 的交互吃掉）
            if bg.clicked() {
                close_request = true;
            }
        });

    if close_request {
        app.folder_closing = true;
    }
    if let Some(a) = launch {
        app.open(&a);
        app.open_folder = None;
        app.folder_closing = false;
    }


}

fn rename_overlay(app: &mut LaunchDeckApp, ctx: &egui::Context, id: &str, current: &str) {
    let screen = ctx.screen_rect();
    let p = pal(app);
    let w = 320.0;
    let pos = egui::pos2(screen.center().x - w / 2.0, screen.center().y - 50.0);
    let mut done = false;
    let mut cancel = false;

    egui::Area::new(egui::Id::new("rename"))
        .fixed_pos(pos)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(p.panel)
                .rounding(12.0)
                .stroke(Stroke::new(1.0_f32, p.panel_border))
                .inner_margin(egui::Margin::same(16.0))
                .show(ui, |ui| {
                    ui.set_width(w - 32.0);
                    ui.label(egui::RichText::new("重命名文件夹").size(13.0).strong());
                    ui.add_space(6.0);
                    let resp = ui.add_sized(
                        [w - 32.0, 26.0],
                        egui::TextEdit::singleline(&mut app.edit_text),
                    );
                    if app.focus_editing {
                        resp.request_focus();
                        app.focus_editing = false;
                    }
                    if resp.lost_focus() {
                        done = true;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            cancel = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("确定").clicked() {
                                done = true;
                            }
                        });
                    });
                });
        });

    if done {
        let name = if app.edit_text.trim().is_empty() {
            "文件夹".to_string()
        } else {
            app.edit_text.trim().to_string()
        };
        app.rename_folder(id, &name);
        app.editing_folder = None;
    } else if cancel {
        app.editing_folder = None;
        app.edit_text = current.to_string();
    }
}

// ---------------------------------------------------------------- 通用确认框

fn confirm_dialog(app: &mut LaunchDeckApp, ctx: &egui::Context) {
    if app.confirm_batch {
        let screen = ctx.screen_rect();
        let p = pal(app);
        let w = 380.0;
        let h = 190.0;
        let pos = screen.center() - egui::vec2(w, h) / 2.0;
        let mut decision: Option<bool> = None;
        egui::Area::new(egui::Id::new("confirm-batch"))
            .fixed_pos(pos)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(p.panel)
                    .rounding(14.0)
                    .stroke(Stroke::new(1.0_f32, p.panel_border))
                    .inner_margin(egui::Margin::same(20.0))
                    .show(ui, |ui| {
                        ui.set_width(w - 40.0);
                        ui.label(egui::RichText::new("确认卸载").size(16.0).strong());
                        ui.add_space(6.0);
                        ui.label(format!(
                            "确定要把选中的 {} 个应用移到废纸篓吗？",
                            app.selected.len()
                        ));
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if ui.button("取消").clicked() {
                                decision = Some(false);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("移到废纸篓")
                                                    .color(Color32::WHITE),
                                            )
                                            .fill(Color32::from_rgb(200, 70, 70)),
                                        )
                                        .clicked()
                                    {
                                        decision = Some(true);
                                    }
                                },
                            );
                        });
                    });
            });
        match decision {
            Some(true) => app.trash_selected(),
            Some(false) => app.confirm_batch = false,
            None => {}
        }
        return;
    }

    let Some(bid) = app.confirm_delete.clone() else { return };
    let name = app
        .apps
        .iter()
        .find(|a| a.bundle_id == bid || a.path.to_string_lossy() == bid)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| bid.clone());
    let path = app
        .apps
        .iter()
        .find(|a| a.bundle_id == bid || a.path.to_string_lossy() == bid)
        .map(|a| a.path.to_string_lossy().to_string())
        .unwrap_or_default();

    let screen = ctx.screen_rect();
    let p = pal(app);
    let w = 380.0;
    let h = 190.0;
    let pos = screen.center() - egui::vec2(w, h) / 2.0;
    let mut decision: Option<bool> = None;

    egui::Area::new(egui::Id::new("confirm"))
        .fixed_pos(pos)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(p.panel)
                .rounding(14.0)
                .stroke(Stroke::new(1.0_f32, p.panel_border))
                .inner_margin(egui::Margin::same(20.0))
                .show(ui, |ui| {
                    ui.set_width(w - 40.0);
                    ui.label(egui::RichText::new("确认卸载").size(16.0).strong());
                    ui.add_space(6.0);
                    ui.label(format!("确定要把「{name}」移到废纸篓吗？"));
                    ui.label(egui::RichText::new(&path).size(10.0).color(p.sub));
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            decision = Some(false);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("移到废纸篓")
                                            .color(Color32::WHITE),
                                    )
                                    .fill(Color32::from_rgb(200, 70, 70)),
                                )
                                .clicked()
                            {
                                decision = Some(true);
                            }
                        });
                    });
                });
        });

    match decision {
        Some(true) => {
            let ar = crate::layout::AppRef {
                bundle_id: bid.clone(),
                name,
                path,
            };
            app.trash(&ar);
            app.confirm_delete = None;
        }
        Some(false) => app.confirm_delete = None,
        None => {}
    }
}

include!("ui_settings.rs");
