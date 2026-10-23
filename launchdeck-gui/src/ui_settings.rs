// ---------------------------------------------------------------- 设置（400×610，1:1 原版布局）
//
// 几何取自原版 2.4.0 设置窗口的 AX 树（窗口原点相对坐标）：
// - 关闭钮 16×16 @ (8,8)；五个标签按钮 74×50，起点 x=19、步距 72、y=33
// - 内容区从 y=90 开始，左右内边距 20；分组卡片 360 宽、圆角 10
// - 分组标题 13px，行内主文字 13px、说明文字 11px，控件右边界 x=380
// - 窗口高度随标签变化：通用 610 / 界面 630 / Apps 482 / 高级 630 / 关于 644

pub const SET_W: f32 = 400.0;
pub const TAB_H: [f32; 5] = [566.0, 630.0, 482.0, 630.0, 644.0];

const WIN_BG: Color32 = Color32::from_rgb(30, 30, 29);
const CARD_BG: Color32 = Color32::from_rgb(37, 37, 36);
const TEXT_1: Color32 = Color32::from_rgb(222, 222, 222);
const TEXT_2: Color32 = Color32::from_rgb(152, 152, 151);
const LIME: Color32 = Color32::from_rgb(188, 224, 117);
const TAB_BLUE: Color32 = Color32::from_rgb(67, 144, 244);

fn settings_window(app: &mut LaunchDeckApp, ctx: &egui::Context) {
    // Esc 关闭设置；点击面板外也关闭（窗口是透明的全屏层，否则会挡住桌面）
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.tab = Tab::Launchpad;
        return;
    }
    let screen = ctx.screen_rect();
    let p = pal(app);
    let set_h = TAB_H[app.settings_section.min(4)];
    let pos = egui::pos2(
        screen.center().x - SET_W / 2.0,
        screen.center().y - set_h / 2.0,
    );

    egui::Area::new(egui::Id::new("settings"))
        .fixed_pos(pos)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(WIN_BG)
                .rounding(12.0)
                .stroke(Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(20, 20, 20, 20)))
                .show(ui, |ui| {
                    ui.set_width(SET_W);
                    ui.set_height(set_h);

                    // 关闭按钮（左上角红点）
                    let (cb, resp) =
                        ui.allocate_exact_size(egui::vec2(SET_W, 24.0), Sense::click_and_drag());
                    if resp.drag_started() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    let c = egui::pos2(cb.min.x + 16.0, cb.min.y + 12.0);
                    ui.painter()
                        .circle_filled(c, 6.0, Color32::from_rgb(255, 95, 86));
                    // 鼠标移上去时，红点内显示 ✕（macOS 习惯）
                    let hovering = ui
                        .ctx()
                        .pointer_hover_pos()
                        .is_some_and(|pos| pos.distance(c) < 10.0);
                    if hovering {
                        ui.painter().text(
                            c,
                            Align2::CENTER_CENTER,
                            "✕",
                            FontId::proportional(9.0),
                            Color32::from_rgba_premultiplied(70, 10, 8, 200),
                        );
                    }
                    if resp.clicked()
                        && ui
                            .ctx()
                            .pointer_interact_pos()
                            .is_some_and(|pos| pos.distance(c) < 9.0)
                    {
                        app.tab = Tab::Launchpad;
                    }

                    let origin = ui.min_rect().min;
                    tabs_row(app, ui, &p);
                    ui.add_space(7.0);

                    let body_h = set_h - 90.0;
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(body_h)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(20.0);
                                ui.vertical(|ui| {
                                    ui.set_width(SET_W - 40.0);
                                    settings_body(app, ui, &p);
                                });
                            });
                            ui.add_space(16.0);
                        });

                });
        });
}

fn tabs_row(app: &mut LaunchDeckApp, ui: &mut egui::Ui, p: &Pal) {
    let tabs = ["通用", "界面", "Apps", "高级", "关于"];
    let (band, band_resp) =
        ui.allocate_exact_size(egui::vec2(SET_W, 50.0), Sense::click_and_drag());
    if band_resp.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }

    for i in 0..5 {
        let r = Rect::from_min_size(
            egui::pos2(band.min.x + 19.0 + i as f32 * 72.0, band.min.y),
            egui::vec2(74.0, 50.0),
        );
        let resp = ui.interact(r, egui::Id::new(("set-tab", i)), Sense::click());
        let selected = app.settings_section == i;
        if selected {
            ui.painter().rect_filled(r, 10.0, CARD_BG);
        }
        let col = if selected { TAB_BLUE } else { TEXT_2 };
        let sym_names = [
            "gearshape",
            "slider.horizontal.3",
            "square.grid.2x2",
            "switch.2",
            "info.circle",
        ];
        let icon_c = egui::pos2(r.center().x, r.min.y + 17.0);
        if let Some(tex) = crate::icons::symbol(ui.ctx(), sym_names[i]) {
            // 构建时按 2× 渲染，这里换算回点
            let sz = tex.size_vec2() / 2.0;
            ui.painter().image(
                tex.id(),
                Rect::from_center_size(icon_c, sz),
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                col,
            );
        } else {
            draw_tab_icon(ui, icon_c, i, col);
        }
        ui.painter().text(
            egui::pos2(r.center().x, r.min.y + 40.0),
            Align2::CENTER_CENTER,
            tabs[i],
            FontId::proportional(10.5),
            col,
        );
        if resp.clicked() && !selected {
            app.settings_section = i;
        }
    }
    let _ = p;
}

/// 用基础图元画五个标签图标（齿轮 / 滑杆 / 网格 / 开关 / 信息）
fn draw_tab_icon(ui: &egui::Ui, c: egui::Pos2, idx: usize, col: Color32) {
    let p = ui.painter();
    let s = Stroke::new(1.4_f32, col);
    match idx {
        0 => {
            p.circle_stroke(c, 4.2, s);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let (dx, dy) = (a.cos(), a.sin());
                p.line_segment(
                    [
                        egui::pos2(c.x + dx * 5.4, c.y + dy * 5.4),
                        egui::pos2(c.x + dx * 7.2, c.y + dy * 7.2),
                    ],
                    s,
                );
            }
        }
        1 => {
            p.circle_stroke(egui::pos2(c.x, c.y - 1.0), 2.6, s);
            for k in 0..6 {
                let a = k as f32 * std::f32::consts::TAU / 6.0;
                let (dx, dy) = (a.cos(), a.sin());
                p.line_segment(
                    [
                        egui::pos2(c.x + dx * 3.6, c.y - 1.0 + dy * 3.6),
                        egui::pos2(c.x + dx * 5.2, c.y - 1.0 + dy * 5.2),
                    ],
                    s,
                );
            }
            p.circle_filled(c, 0.0, col);
        }
        2 => {
            p.rect_stroke(
                Rect::from_center_size(c, egui::vec2(12.0, 12.0)),
                3.5,
                s,
            );
        }
        3 => {
            p.line_segment(
                [
                    egui::pos2(c.x - 5.5, c.y - 3.0),
                    egui::pos2(c.x + 5.5, c.y - 3.0),
                ],
                s,
            );
            p.line_segment(
                [
                    egui::pos2(c.x - 5.5, c.y + 3.0),
                    egui::pos2(c.x + 5.5, c.y + 3.0),
                ],
                s,
            );
            p.circle_filled(egui::pos2(c.x + 2.0, c.y - 3.0), 1.6, col);
            p.circle_filled(egui::pos2(c.x - 2.0, c.y + 3.0), 1.6, col);
        }
        _ => {
            p.circle_stroke(c, 6.0, s);
            p.text(
                c,
                Align2::CENTER_CENTER,
                "i",
                FontId::proportional(9.0),
                col,
            );
        }
    }
}


fn circle_label(ui: &mut egui::Ui, c: egui::Pos2, text: &str, col: Color32) {
    ui.painter()
        .circle_stroke(c, 6.5, Stroke::new(1.0_f32, col));
    ui.painter().text(
        c,
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(9.0),
        col,
    );
}

fn settings_body(app: &mut LaunchDeckApp, ui: &mut egui::Ui, p: &Pal) {
    match app.settings_section {
        0 => settings_general(app, ui, p),
        1 => settings_interface(app, ui, p),
        2 => settings_apps(app, ui, p),
        3 => settings_advanced(app, ui, p),
        _ => settings_about(app, ui, p),
    }
}

fn settings_general(app: &mut LaunchDeckApp, ui: &mut egui::Ui, p: &Pal) {
    heading(ui, "启动", p);
    row(ui, p, "开机自启动", None, |ui| {
        toggle(ui, &mut app.settings.launch_at_login, TAB_BLUE);
    });
    group(ui, 194.0, |ui| {
        grow(ui, p, "快捷键", None, |ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(120.0, 24.0), Sense::hover());
            ui.painter()
                .rect_filled(r, 5.0, Color32::from_rgba_premultiplied(28, 28, 28, 28));
            ui.painter().text(
                egui::pos2(r.min.x + 10.0, r.center().y),
                Align2::LEFT_CENTER,
                "\u{2325}Space",
                FontId::proportional(12.0),
                TEXT_1,
            );
            let x = egui::pos2(r.max.x - 12.0, r.center().y);
            ui.painter().circle_filled(
                x,
                7.0,
                Color32::from_rgba_premultiplied(55, 55, 55, 55),
            );
            ui.painter().text(
                x,
                Align2::CENTER_CENTER,
                "\u{2715}",
                FontId::proportional(7.0),
                TEXT_1,
            );
        });
        grow(ui, p, "系统 F4 键", None, |ui| {
            perm_button(ui, p, "Accessibility");
        });
        grow(ui, p, "触控板", None, |ui| {
            perm_button(ui, p, "Accessibility");
        });
        grow(ui, p, "触发角", None, |ui| {
            egui::ComboBox::from_id_source("hotcorner")
                .selected_text(app.settings.hot_corner.label())
                .width(72.0)
                .show_ui(ui, |ui| {
                    for c in [
                        HotCorner::None,
                        HotCorner::TopLeft,
                        HotCorner::TopRight,
                        HotCorner::BottomLeft,
                        HotCorner::BottomRight,
                    ] {
                        if ui
                            .selectable_value(&mut app.settings.hot_corner, c, c.label())
                            .clicked()
                        {
                            crate::native::set_hot_corner(c);
                            let _ = launchdeck_core::save_settings(&app.settings);
                        }
                    }
                });
        });
        grow(ui, p, "多选", None, |ui| {
            ui.label(
                egui::RichText::new("按住 Command + 单击或拖拽")
                    .size(13.0)
                    .color(TEXT_1),
            );
        });
    });

    ui.add_space(8.0);
    heading(ui, "外观", p);
    row(ui, p, "显示菜单栏图标", None, |ui| {
        let mut v = app.settings.show_menu_bar_icon;
        if toggle(ui, &mut v, TAB_BLUE) {
            app.settings.show_menu_bar_icon = v;
            if v {
                crate::native::install_menu_bar();
            } else {
                crate::native::remove_menu_bar();
            }
            let _ = launchdeck_core::save_settings(&app.settings);
        }
    });
    // 应用图标：62 高卡片，右侧四个 32×42 图标样式
    let r = card(ui, 62.0);
    ui.painter().text(
        egui::pos2(r.min.x + 10.0, r.center().y),
        Align2::LEFT_CENTER,
        "应用图标",
        FontId::proportional(13.0),
        TEXT_1,
    );
    let ic = egui::pos2(r.min.x + 87.0, r.center().y - 6.0);
    ui.painter().circle_stroke(ic, 6.5, Stroke::new(1.0_f32, TEXT_2));
    ui.painter().text(
        ic,
        Align2::CENTER_CENTER,
        "i",
        FontId::proportional(9.0),
        TEXT_2,
    );
    for (i, kind) in [
        IconStyle::Rounded,
        IconStyle::Circle,
        IconStyle::Square,
        IconStyle::Custom,
    ]
    .into_iter()
    .enumerate()
    {
        let x = r.min.x + 174.0 + i as f32 * 48.0;
        let tile = Rect::from_min_size(egui::pos2(x, r.min.y + 10.0), egui::vec2(32.0, 32.0));
        let resp = ui.interact(
            Rect::from_min_size(egui::pos2(x, r.min.y + 10.0), egui::vec2(32.0, 42.0)),
            egui::Id::new(("icon-style", i)),
            Sense::click(),
        );
        let selected = app.settings.icon_style == kind;
        // 原版的四种预览图（随包资源，直接从原版界面裁剪）
        if let Some(tex) = crate::icons::asset(ui.ctx(), "iconstyles", &format!("style{i}")) {
            ui.painter().image(
                tex.id(),
                tile,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            ui.painter().rect_filled(tile, 8.0, Color32::from_rgb(58, 58, 57));
        }
        if selected {
            // 原版选中态：图标下方一个小蓝点
            ui.painter().circle_filled(
                egui::pos2(tile.center().x, tile.max.y + 11.0),
                2.2,
                Color32::from_rgb(10, 132, 255),
            );
        }
        if resp.clicked() {
            app.settings.icon_style = kind;
            let idx = match kind {
                IconStyle::Rounded => 0,
                IconStyle::Circle => 1,
                IconStyle::Square => 2,
                IconStyle::Custom => 3,
            };
            crate::native::set_app_icon(idx);
            let _ = launchdeck_core::save_settings(&app.settings);
        }
    }
    ui.add_space(10.0);
}

/// 原版的小号绿色胶囊（表示「需要权限/已启用」）
fn pill(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 9.0), Sense::hover());
    ui.painter().rect_filled(r, 4.5, LIME);
}

fn perm_button(ui: &mut egui::Ui, p: &Pal, pane: &str) {
    let _ = p;
    let (r, resp) = ui.allocate_exact_size(egui::vec2(76.0, 24.0), Sense::click());
    if resp.clicked() {
        let _ = std::process::Command::new("/usr/bin/open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_{pane}"
            ))
            .spawn();
    }
    ui.painter()
        .rect_filled(r, 6.0, Color32::from_rgb(48, 48, 47));
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        "授予权限",
        FontId::proportional(12.0),
        Color32::from_rgb(110, 110, 109),
    );
}

fn icon_styles(app: &mut LaunchDeckApp, ui: &mut egui::Ui) {
    let items = [
        (IconStyle::Rounded, "圆角矩形"),
        (IconStyle::Circle, "圆形"),
        (IconStyle::Square, "方形"),
        (IconStyle::Custom, "自定义"),
    ];
    ui.horizontal(|ui| {
    for (style, _) in items.iter() {
        let (r, resp) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), Sense::click());
        let fill = Color32::from_rgb(40, 42, 48);
        ui.painter().rect_filled(r, 8.0, fill);
        ui.painter().circle_filled(
            r.center(),
            4.0,
            Color32::from_rgb(160, 200, 255),
        );
        if app.settings.icon_style == *style {
            ui.painter().rect_stroke(
                r,
                8.0,
                Stroke::new(2.0_f32, BLUE),
            );
            ui.painter()
                .circle_filled(egui::pos2(r.center().x, r.max.y + 8.0), 2.0, BLUE);
        }
        if resp.clicked() {
            app.settings.icon_style = *style;
            let _ = launchdeck_core::save_settings(&app.settings);
        }
    }
    });
}

fn settings_interface(app: &mut LaunchDeckApp, ui: &mut egui::Ui, p: &Pal) {
    // 显示模式（原版：一张 133 高的卡片，右侧两个 78×74 缩略图）
    let r = card(ui, 133.0);
    ui.painter().text(
        egui::pos2(r.min.x + 10.0, r.min.y + 9.0 + 8.0),
        Align2::LEFT_CENTER,
        "显示模式",
        FontId::proportional(13.0),
        TEXT_1,
    );
    ui.allocate_ui_at_rect(
        Rect::from_min_size(
            egui::pos2(r.min.x + 182.0, r.min.y + 10.0),
            egui::vec2(168.0, 74.0),
        ),
        |ui| {
            ui.horizontal(|ui| {
                display_thumb(app, ui, DisplayMode::Fullscreen, "全屏");
                ui.add_space(12.0);
                display_thumb(app, ui, DisplayMode::Window, "窗口");
            });
        },
    );
    ui.add_space(10.0);
    row(ui, p, "隐藏程序坞", None, |ui| {
        let mut hidden = !app.settings.show_dock_icon;
        if toggle(ui, &mut hidden, TAB_BLUE) {
            app.settings.show_dock_icon = !hidden;
            crate::native::set_dock_icon_visible(!hidden);
            let _ = launchdeck_core::save_settings(&app.settings);
        }
    });

    ui.add_space(8.0);
    heading(ui, "布局", p);
    row(ui, p, "排序方式", None, |ui| {
        egui::ComboBox::from_id_source("sortmode")
            .selected_text(app.layout.sort_mode.label())
            .width(86.0)
            .show_ui(ui, |ui| {
                for m in [SortMode::Custom, SortMode::Name, SortMode::LastUsed] {
                    if ui
                        .selectable_value(&mut app.layout.sort_mode, m, m.label())
                        .clicked()
                    {
                        app.reorganize();
                    }
                }
            });
    });
    row(ui, p, "浏览样式", None, |ui| {
        let mut idx = if app.layout.browsing_style == crate::layout::BrowsingStyle::Paged {
            0
        } else {
            1
        };
        if segmented(ui, &["分页", "滚动"], &mut idx) {
            app.layout.browsing_style = if idx == 0 {
                crate::layout::BrowsingStyle::Paged
            } else {
                crate::layout::BrowsingStyle::Scroll
            };
            let _ = crate::layout_store::save(&app.layout);
        }
    });
    row(ui, p, "网格布局", None, |ui| {
        let b = stepper(ui, &mut app.layout.rows, 3, 8);
        ui.label(egui::RichText::new("×").size(10.0).color(TEXT_2));
        let a = stepper(ui, &mut app.layout.columns, 4, 12);
        if a || b {
            crate::layout_store::reflow(&mut app.layout);
            let _ = crate::layout_store::save(&app.layout);
        }
    });
    row(
        ui,
        p,
        "整理空位",
        Some("根据当前行列布局，将 App 向前整理以填补空位。"),
        |ui| {
            if small_button(ui, "重新整理", p) {
                app.reorganize();
            }
        },
    );
    row(ui, p, "隐藏名称", None, |ui| {
        toggle(ui, &mut app.layout.hide_names, TAB_BLUE);
    });

    ui.add_space(8.0);
    heading(ui, "背景", p);
    row(ui, p, "类型", None, |ui| {
        let mut idx = if app.settings.background == BackgroundType::Wallpaper {
            0
        } else {
            1
        };
        if segmented(ui, &["系统壁纸", "毛玻璃"], &mut idx) {
            app.settings.background = if idx == 0 {
                BackgroundType::Wallpaper
            } else {
                BackgroundType::Frosted
            };
            let _ = launchdeck_core::save_settings(&app.settings);
        }
    });
}

fn display_thumb(app: &mut LaunchDeckApp, ui: &mut egui::Ui, mode: DisplayMode, label: &str) {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(78.0, 58.0), Sense::click());
    let selected = app.settings.display_mode == mode;
    ui.painter().rect_filled(
        r,
        6.0,
        Color32::from_rgba_unmultiplied(255, 255, 255, 18),
    );
    // 小屏示意
    let mini = Rect::from_center_size(
        egui::pos2(r.center().x, r.min.y + 30.0),
        if mode == DisplayMode::Fullscreen {
            egui::vec2(58.0, 34.0)
        } else {
            egui::vec2(46.0, 30.0)
        },
    );
    ui.painter()
        .rect_filled(mini, 3.0, Color32::from_rgba_unmultiplied(90, 160, 255, 180));
    // 缩略图里的小图标点
    for i in 0..6 {
        let cx = mini.min.x + 8.0 + (i % 3) as f32 * 14.0;
        let cy = mini.min.y + 7.0 + (i / 3) as f32 * 13.0;
        ui.painter().rect_filled(
            Rect::from_min_size(egui::pos2(cx, cy), egui::vec2(7.0, 7.0)),
            1.5,
            Color32::from_white_alpha(200),
        );
    }
    if selected {
        ui.painter()
            .rect_stroke(r, 6.0, Stroke::new(1.5_f32, BLUE));
    }
    ui.painter().text(
        egui::pos2(r.center().x, r.max.y + 12.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(10.5),
        Color32::from_gray(220),
    );
    if resp.clicked() {
        app.set_display_mode(mode);
    }
    ui.allocate_space(egui::vec2(0.0, 14.0));
}

fn settings_apps(app: &mut LaunchDeckApp, ui: &mut egui::Ui, p: &Pal) {
    heading(ui, "应用来源", p);
    ui.horizontal(|ui| {
        ui.add_space(0.0);
        ui.vertical(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(360.0, 81.0), Sense::hover());
            ui.painter().rect_filled(r, 10.0, CARD_BG);
            ui.painter().text(
                egui::pos2(r.min.x + 12.0, r.min.y + 16.0),
                Align2::LEFT_CENTER,
                "▣ 应用程序",
                FontId::proportional(11.5),
                p.text,
            );
            ui.painter().text(
                egui::pos2(r.min.x + 24.0, r.min.y + 34.0),
                Align2::LEFT_CENTER,
                "默认路径",
                FontId::proportional(9.5),
                p.sub,
            );
            ui.painter().text(
                egui::pos2(r.min.x + 12.0, r.max.y - 14.0),
                Align2::LEFT_CENTER,
                "＋   －",
                FontId::proportional(11.0),
                p.sub,
            );
        });
    });

    heading(ui, "隐藏的应用", p);
    row(ui, p, "在搜索结果中隐藏应用", None, |ui| {
        toggle(ui, &mut app.settings.hide_hidden_in_search, TAB_BLUE);
    });
    ui.add_space(2.0);
    let hidden: Vec<String> = app.layout.hidden.iter().cloned().collect();
    if hidden.is_empty() {
        let (r, _) = ui.allocate_exact_size(egui::vec2(360.0, 140.0), Sense::hover());
        ui.painter().rect_filled(r, 10.0, CARD_BG);
        ui.painter().text(
            r.center() - egui::vec2(0.0, 8.0),
            Align2::CENTER_CENTER,
            "暂无应用",
            FontId::proportional(11.5),
            p.text,
        );
        ui.painter().text(
            r.center() + egui::vec2(0.0, 10.0),
            Align2::CENTER_CENTER,
            "右键点击应用即可隐藏",
            FontId::proportional(9.5),
            p.sub,
        );
    } else {
        let mut unhide: Option<String> = None;
        for bid in &hidden {
            ui.horizontal(|ui| {
                let name = app
                    .apps
                    .iter()
                    .find(|a| &a.bundle_id == bid)
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| bid.clone());
                ui.label(egui::RichText::new(name).size(11.0).color(p.text));
                if ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| small_button(ui, "取消隐藏", p),
                ).inner {
                    unhide = Some(bid.clone());
                }
            });
        }
        if let Some(b) = unhide {
            app.unhide(&b);
        }
    }
}

fn settings_advanced(app: &mut LaunchDeckApp, ui: &mut egui::Ui, p: &Pal) {
    heading(ui, "行为", p);
    row(ui, p, "启动台显示在", None, |ui| {
        egui::ComboBox::from_id_source("screen")
            .selected_text(app.settings.show_on.label())
            .width(86.0)
            .show_ui(ui, |ui| {
                for v in [
                    ScreenChoice::Main,
                    ScreenChoice::Cursor,
                    ScreenChoice::Dock,
                    ScreenChoice::Active,
                ] {
                    ui.selectable_value(&mut app.settings.show_on, v, v.label());
                }
            });
    });
    row(
        ui,
        p,
        "回到主页面",
        Some("重新打开时，返回首页或延续上次位置"),
        |ui| {
            egui::ComboBox::from_id_source("return")
                .selected_text(app.settings.return_home.label())
                .width(98.0)
                .show_ui(ui, |ui| {
                    for v in [
                        ReturnHome::Never,
                        ReturnHome::FiveMinutes,
                        ReturnHome::OneMinute,
                        ReturnHome::ThirtySeconds,
                        ReturnHome::FifteenSeconds,
                        ReturnHome::FiveSeconds,
                    ] {
                        ui.selectable_value(&mut app.settings.return_home, v, v.label());
                    }
                });
        },
    );
    row(ui, p, "按住 Option 进入卸载模式", None, |ui| {
        toggle(ui, &mut app.settings.option_uninstall, TAB_BLUE);
    });

    heading(ui, "布局", p);
    row(
        ui,
        p,
        "系统 Launchpad 数据",
        Some("恢复系统启动台布局数据"),
        |ui| {
            if small_button(ui, "恢复", p) {
                app.import_original_layout();
            }
        },
    );

    // 布局数据备份：97 高卡片，说明在上，按钮在右下
    {
        let r = card(ui, 97.0);
        ui.painter().text(
            egui::pos2(r.min.x + 10.0, r.min.y + 16.0),
            Align2::LEFT_CENTER,
            "布局数据备份",
            FontId::proportional(13.0),
            TEXT_1,
        );
        ui.painter().text(
            egui::pos2(r.min.x + 10.0, r.min.y + 34.0),
            Align2::LEFT_CENTER,
            "导入或导出 LaunchDeck 布局，用于备份、恢复或在设备之间迁移",
            FontId::proportional(11.0),
            TEXT_2,
        );
        let b1 = Rect::from_min_size(
            egui::pos2(r.max.x - 112.0, r.min.y + 63.0),
            egui::vec2(50.0, 24.0),
        );
        let b2 = Rect::from_min_size(
            egui::pos2(r.max.x - 56.0, r.min.y + 63.0),
            egui::vec2(50.0, 24.0),
        );
        if button_at(ui, b1, "导入") {
            app.import_layout_dialog();
        }
        if button_at(ui, b2, "导出") {
            app.export_layout_dialog();
        }
        ui.add_space(10.0);
    }

    // 整理布局：整张卡片就是一个按钮
    {
        let r = card(ui, 52.0);
        let hit = r.shrink2(egui::vec2(10.0, 10.0));
        let resp = ui.interact(hit, egui::Id::new("tidy-layout"), Sense::click());
        ui.painter().text(
            egui::pos2(r.min.x + 10.0, r.min.y + 16.0),
            Align2::LEFT_CENTER,
            "整理布局",
            FontId::proportional(13.0),
            TEXT_1,
        );
        ui.painter().text(
            egui::pos2(r.min.x + 10.0, r.min.y + 34.0),
            Align2::LEFT_CENTER,
            "根据所选规则重新整理布局",
            FontId::proportional(11.0),
            TEXT_2,
        );
        if resp.hovered() {
            ui.painter().rect_stroke(
                hit,
                8.0,
                Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(40, 40, 40, 40)),
            );
        }
        if resp.clicked() {
            app.reorganize();
        }
        ui.add_space(10.0);
    }

    heading(ui, "更新", p);
    row(ui, p, "自动检查更新", None, |ui| {
        toggle(ui, &mut app.settings.auto_update, TAB_BLUE);
    });
    row(ui, p, "自动下载更新", None, |ui| {
        toggle(ui, &mut app.settings.auto_download, TAB_BLUE);
    });
}

/// 指定矩形上的小按钮
fn button_at(ui: &mut egui::Ui, r: Rect, text: &str) -> bool {
    let resp = ui.interact(r, egui::Id::new(("btn", text, r.min.x as i32)), Sense::click());
    let bg = if resp.hovered() {
        Color32::from_rgb(58, 58, 57)
    } else {
        Color32::from_rgb(48, 48, 47)
    };
    ui.painter().rect_filled(r, 6.0, bg);
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.0),
        TEXT_1,
    );
    resp.clicked()
}

fn settings_about(_app: &mut LaunchDeckApp, ui: &mut egui::Ui, p: &Pal) {
    let _ = p;
    let win = ui.ctx().screen_rect();
    let o = win.min;
    let at = |x: f32, y: f32| egui::pos2(o.x + x, o.y + y);
    let painter = ui.painter().clone();

    // Logo（取自原版「关于」页，已去掉 Free 徽标）
    let logo = Rect::from_min_size(at(152.0, 144.0), egui::vec2(96.0, 96.0));
    if let Some(tex) = crate::icons::asset(ui.ctx(), "ui", "logo") {
        painter.image(
            tex.id(),
            logo,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    painter.text(
        at(200.0, 273.0),
        Align2::CENTER_CENTER,
        "LaunchDeck",
        FontId::proportional(26.0),
        TEXT_1,
    );
    painter.text(
        at(200.0, 305.0),
        Align2::CENTER_CENTER,
        launchdeck_core::VERSION,
        FontId::proportional(12.0),
        TEXT_2,
    );
    painter.text(
        at(200.0, 331.0),
        Align2::CENTER_CENTER,
        "用 Rust 实现的 macOS 启动台",
        FontId::proportional(11.5),
        TEXT_2,
    );

    // 信息行（原版这里放的是官网/社交账号，本仓库不冒充原作者，改为项目信息）
    let links: [(&str, &str); 4] = [
        ("开源协议", "MIT"),
        ("界面框架", "Rust + egui"),
        ("系统要求", "macOS 13+"),
        ("数据目录", "Application Support/LaunchDeck"),
    ];
    for (i, (k, v)) in links.iter().enumerate() {
        let y = 372.0 + i as f32 * 32.0;
        painter.text(
            at(30.0, y),
            Align2::LEFT_CENTER,
            *k,
            FontId::proportional(13.0),
            TEXT_1,
        );
        painter.text(
            at(370.0, y),
            Align2::RIGHT_CENTER,
            *v,
            FontId::proportional(13.0),
            TEXT_2,
        );
    }

    // 开源声明
    painter.text(
        at(200.0, 511.0),
        Align2::CENTER_CENTER,
        "本软件按 MIT 协议开源，与原版商业软件无关",
        FontId::proportional(12.0),
        TEXT_2,
    );

    painter.text(
        at(200.0, 551.0),
        Align2::CENTER_CENTER,
        "© 2026 LaunchDeck contributors · MIT License",
        FontId::proportional(10.5),
        TEXT_2,
    );

    // 底部 更新 / 反馈
    let seg = Rect::from_min_size(at(280.0, 600.0), egui::vec2(100.0, 24.0));
    painter.rect_filled(seg, 6.0, Color32::from_rgb(48, 48, 47));
    for (i, t) in ["更新", "反馈"].iter().enumerate() {
        let r = Rect::from_min_size(
            egui::pos2(seg.min.x + i as f32 * 50.0, seg.min.y),
            egui::vec2(50.0, 24.0),
        );
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            *t,
            FontId::proportional(12.0),
            TEXT_1,
        );
    }
}

// ---------------------------------------------------------------- 小控件

fn heading_toggle(ui: &mut egui::Ui, p: &Pal, text: &str, value: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text).size(10.5).color(p.sub));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            toggle(ui, value, GREEN);
        });
    });
}

fn heading(ui: &mut egui::Ui, text: &str, p: &Pal) {
    let _ = p;
    ui.add_space(12.0);
    let (r, _) = ui.allocate_exact_size(egui::vec2(360.0, 16.0), Sense::hover());
    ui.painter().text(
        egui::pos2(r.min.x + 10.0, r.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(13.0),
        TEXT_1,
    );
    ui.add_space(2.0);
}

/// 分组卡片：360 宽、圆角 10、底色 #252524
fn card(ui: &mut egui::Ui, h: f32) -> Rect {
    let (r, _) = ui.allocate_exact_size(egui::vec2(360.0, h), Sense::hover());
    ui.painter().rect_filled(r, 10.0, CARD_BG);
    r
}

/// 一张卡里放多行（原版：行间用 2px 窗口底色分隔）
fn group(ui: &mut egui::Ui, h: f32, body: impl FnOnce(&mut egui::Ui)) {
    let r = card(ui, h);
    let mut child = ui.child_ui(
        r.shrink2(egui::vec2(0.0, 0.0)),
        egui::Layout::top_down(egui::Align::Min),
        None,
    );
    child.set_width(r.width());
    child.spacing_mut().item_spacing.y = 0.0;
    body(&mut child);
    ui.add_space(2.0);
}


/// 组内单行：标签后紧跟绿色胶囊，控件右对齐
fn grow_pill(
    ui: &mut egui::Ui,
    p: &Pal,
    label: &str,
    ok: bool,
    control: impl FnOnce(&mut egui::Ui),
) {
    let _ = p;
    let h = 40.0;
    let (r, _) = ui.allocate_exact_size(egui::vec2(360.0, h), Sense::hover());
    let font = FontId::proportional(13.0);
    ui.painter().text(
        egui::pos2(r.min.x + 10.0, r.center().y),
        Align2::LEFT_CENTER,
        label,
        font.clone(),
        TEXT_1,
    );
    let w = ui
        .fonts(|f| f.layout_no_wrap(label.to_owned(), font, TEXT_1).size().x);
    let px = r.min.x + 10.0 + w + 16.0;
    ui.painter().rect_filled(
        Rect::from_center_size(egui::pos2(px, r.center().y), egui::vec2(16.0, 9.0)),
        4.5,
        if ok {
            LIME
        } else {
            Color32::from_rgb(90, 90, 89)
        },
    );
    ui.allocate_ui_at_rect(r.shrink2(egui::vec2(10.0, 0.0)), |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            control(ui);
        });
    });
    let y = r.max.y + 1.0;
    ui.painter().line_segment(
        [egui::pos2(r.min.x, y), egui::pos2(r.max.x, y)],
        Stroke::new(1.0_f32, WIN_BG),
    );
}

/// 组内单行（不带自己卡片），行高 40，行间画分隔线
fn grow(
    ui: &mut egui::Ui,
    p: &Pal,
    label: &str,
    sub: Option<&str>,
    control: impl FnOnce(&mut egui::Ui),
) {
    let _ = p;
    let h = 40.0;
    let (r, _) = ui.allocate_exact_size(egui::vec2(360.0, h), Sense::hover());
    if let Some(s) = sub {
        ui.painter().text(
            egui::pos2(r.min.x + 10.0, r.center().y - 9.0),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            TEXT_1,
        );
        ui.painter().text(
            egui::pos2(r.min.x + 10.0, r.center().y + 10.0),
            Align2::LEFT_CENTER,
            s,
            FontId::proportional(11.0),
            TEXT_2,
        );
    } else {
        ui.painter().text(
            egui::pos2(r.min.x + 10.0, r.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            TEXT_1,
        );
    }
    ui.allocate_ui_at_rect(r.shrink2(egui::vec2(10.0, 0.0)), |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            control(ui);
        });
    });
    // 行分隔
    let y = r.max.y + 1.0;
    ui.painter().line_segment(
        [
            egui::pos2(r.min.x, y),
            egui::pos2(r.max.x, y),
        ],
        Stroke::new(1.0_f32, WIN_BG),
    );
}

fn row(
    ui: &mut egui::Ui,
    p: &Pal,
    label: &str,
    sub: Option<&str>,
    control: impl FnOnce(&mut egui::Ui),
) {
    let _ = p;
    let h = if sub.is_some() { 52.0 } else { 38.0 };
    let r = card(ui, h);

    if let Some(s) = sub {
        ui.painter().text(
            egui::pos2(r.min.x + 10.0, r.center().y - 9.0),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            TEXT_1,
        );
        ui.painter().text(
            egui::pos2(r.min.x + 10.0, r.center().y + 10.0),
            Align2::LEFT_CENTER,
            s,
            FontId::proportional(11.0),
            TEXT_2,
        );
    } else {
        ui.painter().text(
            egui::pos2(r.min.x + 10.0, r.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            TEXT_1,
        );
    }

    ui.allocate_ui_at_rect(r.shrink2(egui::vec2(10.0, 0.0)), |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            control(ui);
        });
    });
    ui.add_space(2.0);
}

/// 36×20 开关，返回是否改变
fn toggle(ui: &mut egui::Ui, value: &mut bool, on_color: Color32) -> bool {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(36.0, 20.0), Sense::click());
    let on = *value;
    ui.painter().rect_filled(
        r,
        10.0,
        if on {
            on_color
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 40)
        },
    );
    let knob_x = if on { r.max.x - 10.0 } else { r.min.x + 10.0 };
    ui.painter()
        .circle_filled(egui::pos2(knob_x, r.center().y), 8.0, Color32::WHITE);
    if resp.clicked() {
        *value = !*value;
        true
    } else {
        false
    }
}

fn segmented(ui: &mut egui::Ui, options: &[&str], selected: &mut usize) -> bool {
    let w = 47.0 * options.len() as f32 + 8.0;
    let (r, _) = ui.allocate_exact_size(egui::vec2(w, 26.0), Sense::hover());
    ui.painter()
        .rect_filled(r, 7.0, Color32::from_rgba_unmultiplied(0, 0, 0, 60));
    let mut changed = false;
    for (i, opt) in options.iter().enumerate() {
        let ir = Rect::from_min_size(
            egui::pos2(r.min.x + 3.0 + i as f32 * 47.0, r.min.y + 3.0),
            egui::vec2(44.0, 20.0),
        );
        let resp = ui.interact(ir, egui::Id::new(("seg", opt)), Sense::click());
        if *selected == i {
            ui.painter()
                .rect_filled(ir, 5.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40));
        }
        ui.painter().text(
            ir.center(),
            Align2::CENTER_CENTER,
            *opt,
            FontId::proportional(10.5),
            Color32::from_gray(235),
        );
        if resp.clicked() && *selected != i {
            *selected = i;
            changed = true;
        }
    }
    changed
}

fn stepper(ui: &mut egui::Ui, value: &mut usize, min: usize, max: usize) -> bool {
    let (r, _) = ui.allocate_exact_size(egui::vec2(58.0, 24.0), Sense::hover());
    ui.painter()
        .rect_filled(r, 6.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20));
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        value.to_string(),
        FontId::proportional(11.0),
        Color32::from_gray(235),
    );
    let minus = Rect::from_min_size(r.min + egui::vec2(2.0, 2.0), egui::vec2(16.0, 20.0));
    let plus = Rect::from_min_size(
        egui::pos2(r.max.x - 18.0, r.min.y + 2.0),
        egui::vec2(16.0, 20.0),
    );
    let mr = ui.interact(minus, egui::Id::new(("step-", r.min.x as i32)), Sense::click());
    let pr = ui.interact(plus, egui::Id::new(("step+", r.min.x as i32)), Sense::click());
    ui.painter().text(
        minus.center(),
        Align2::CENTER_CENTER,
        "−",
        FontId::proportional(11.0),
        Color32::from_gray(200),
    );
    ui.painter().text(
        plus.center(),
        Align2::CENTER_CENTER,
        "+",
        FontId::proportional(11.0),
        Color32::from_gray(200),
    );
    let mut changed = false;
    if mr.clicked() && *value > min {
        *value -= 1;
        changed = true;
    }
    if pr.clicked() && *value < max {
        *value += 1;
        changed = true;
    }
    changed
}

fn small_button(ui: &mut egui::Ui, text: &str, _p: &Pal) -> bool {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(52.0, 24.0), Sense::click());
    ui.painter().rect_filled(
        r,
        6.0,
        Color32::from_rgba_unmultiplied(255, 255, 255, if resp.hovered() { 44 } else { 26 }),
    );
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(10.5),
        Color32::from_gray(235),
    );
    resp.clicked()
}

fn separator(ui: &mut egui::Ui, p: &Pal, inset: f32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(SET_W, 1.0), Sense::hover());
    ui.painter().rect_filled(
        Rect::from_min_max(
            egui::pos2(r.min.x + inset, r.min.y),
            egui::pos2(r.max.x - inset, r.max.y),
        ),
        0.0,
        p.separator,
    );
}

// ---------------------------------------------------------------- 卸载窗口

fn uninstall_window(app: &mut LaunchDeckApp, ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    let p = pal(app);
    let w = 520.0;
    let h = 560.0;
    let pos = screen.center() - egui::vec2(w, h) / 2.0;

    egui::Area::new(egui::Id::new("uninstall"))
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
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("卸载应用").size(18.0).strong());
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.button("完成").clicked() {
                                    app.tab = Tab::Launchpad;
                                }
                            },
                        );
                    });
                    ui.label(
                        egui::RichText::new("勾选后点击「移到废纸篓」")
                            .size(10.0)
                            .color(p.sub),
                    );
                    ui.add_space(6.0);

                    let mut list: Vec<(String, String, String, u64)> = app
                        .apps
                        .iter()
                        .map(|a| {
                            (
                                if a.bundle_id.is_empty() {
                                    a.path.to_string_lossy().to_string()
                                } else {
                                    a.bundle_id.clone()
                                },
                                a.name.clone(),
                                a.path.to_string_lossy().to_string(),
                                a.size,
                            )
                        })
                        .collect();
                    list.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(h - 130.0)
                        .show(ui, |ui| {
                            for (bid, name, path, size) in &list {
                                ui.horizontal(|ui| {
                                    let mut checked = app.selected.contains(bid);
                                    if ui.checkbox(&mut checked, name).changed() {
                                        if checked {
                                            app.selected.insert(bid.clone());
                                        } else {
                                            app.selected.remove(bid);
                                        }
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(format_bytes(*size))
                                                    .size(9.0)
                                                    .color(p.sub),
                                            );
                                        },
                                    );
                                });
                                ui.label(
                                    egui::RichText::new(path)
                                        .size(8.5)
                                        .color(p.sub),
                                );
                            }
                        });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                app.selected.len() == 1,
                                egui::Button::new("移到废纸篓"),
                            )
                            .clicked()
                        {
                            if let Some(bid) = app.selected.iter().next().cloned() {
                                app.confirm_delete = Some(bid);
                            }
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "已选 {} / {}",
                                app.selected.len(),
                                app.apps.len()
                            ))
                            .size(10.0)
                            .color(p.sub),
                        );
                    });
                });
        });
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}
