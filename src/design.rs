use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, RichText, Stroke, Vec2};
pub const BG: Color32 = Color32::from_rgb(15, 17, 21);
pub const SIDEBAR: Color32 = Color32::from_rgb(19, 21, 26);
pub const SURFACE: Color32 = Color32::from_rgb(25, 28, 34);
pub const RAISED: Color32 = Color32::from_rgb(33, 37, 45);
pub const LINE: Color32 = Color32::from_rgb(46, 51, 61);
pub const TEXT: Color32 = Color32::from_rgb(235, 238, 244);
pub const MUTED: Color32 = Color32::from_rgb(153, 162, 179);
pub const FAINT: Color32 = Color32::from_rgb(113, 124, 144);
pub const BLUE: Color32 = Color32::from_rgb(157, 188, 248);
pub const GREEN: Color32 = Color32::from_rgb(133, 209, 173);
pub const RED: Color32 = Color32::from_rgb(241, 151, 158);
pub const AMBER: Color32 = Color32::from_rgb(226, 194, 139);
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}
pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "body".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "medium".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Medium.ttf")).into(),
    );
    fonts.font_data.insert(
        "display".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Light.ttf")).into(),
    );
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "body".into());
    for name in ["medium", "display"] {
        let mut family = vec![name.to_string()];
        family.extend(fonts.families[&FontFamily::Proportional].clone());
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = SURFACE;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.extreme_bg_color = Color32::from_rgb(18, 20, 25);
    style.visuals.selection.bg_fill = Color32::from_rgb(44, 60, 88);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, BLUE);
    style.visuals.window_corner_radius = 18.into();
    style.visuals.window_stroke = Stroke::new(1.0_f32, LINE);
    style.visuals.widgets.noninteractive.bg_fill = SURFACE;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    for (w, bg) in [
        (&mut style.visuals.widgets.inactive, RAISED),
        (
            &mut style.visuals.widgets.hovered,
            Color32::from_rgb(46, 52, 63),
        ),
        (
            &mut style.visuals.widgets.active,
            Color32::from_rgb(55, 64, 81),
        ),
    ] {
        w.corner_radius = 8.into();
        w.bg_fill = bg;
        w.weak_bg_fill = bg;
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.expansion = 0.0;
    }
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(16.0, 10.0);
    style.spacing.interact_size.y = 36.0;
    style.spacing.combo_width = 150.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, regular(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, medium(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, regular(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, medium(22.0));
    style.visuals.hyperlink_color = BLUE;
    style.animation_time = 0.12;
    ctx.set_style(style);
}
#[derive(Clone, Copy)]
pub enum Glyph {
    Import,
    Grid,
    Activity,
    Settings,
    Search,
    Check,
    Close,
    Package,
    Folder,
    Refresh,
    Info,
    Trash,
    File,
    Shield,
}
pub fn glyph(p: &egui::Painter, rect: Rect, g: Glyph, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) / 24.0;
    let point = |x: f32, y: f32| c + Vec2::new(x * s, y * s);
    let stroke = Stroke::new(1.55_f32 * s, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
    };
    let poly = |v: &[(f32, f32)]| {
        p.add(egui::Shape::line(
            v.iter().map(|(x, y)| point(*x, *y)).collect(),
            stroke,
        ));
    };
    match g {
        Glyph::Import => {
            line((0.0, -8.0), (0.0, 4.0));
            poly(&[(-4.0, 0.0), (0.0, 4.0), (4.0, 0.0)]);
            poly(&[(-8.0, 3.0), (-8.0, 9.0), (8.0, 9.0), (8.0, 3.0)]);
        }
        Glyph::Grid => {
            for (x, y) in [(-8.0, -8.0), (2.0, -8.0), (-8.0, 2.0), (2.0, 2.0)] {
                p.rect_stroke(
                    Rect::from_min_size(point(x, y), Vec2::splat(6.0 * s)),
                    1.5 * s,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        Glyph::Activity => {
            line((-8.0, -6.0), (8.0, -6.0));
            line((-8.0, 0.0), (5.0, 0.0));
            line((-8.0, 6.0), (8.0, 6.0));
        }
        Glyph::Settings => {
            for (y, x) in [(-6.0, -3.0), (0.0, 4.0), (6.0, -1.0)] {
                line((-8.0, y), (8.0, y));
                p.circle_filled(point(x, y), 2.7 * s, SIDEBAR);
                p.circle_stroke(point(x, y), 2.7 * s, stroke);
            }
        }
        Glyph::Search => {
            p.circle_stroke(point(-2.0, -2.0), 6.0 * s, stroke);
            line((3.0, 3.0), (8.0, 8.0));
        }
        Glyph::Check => poly(&[(-7.0, 0.0), (-2.0, 5.0), (8.0, -5.0)]),
        Glyph::Close => {
            line((-5.0, -5.0), (5.0, 5.0));
            line((5.0, -5.0), (-5.0, 5.0));
        }
        Glyph::Package => {
            poly(&[
                (-8.0, -4.0),
                (0.0, -9.0),
                (8.0, -4.0),
                (8.0, 5.0),
                (0.0, 10.0),
                (-8.0, 5.0),
                (-8.0, -4.0),
                (0.0, 1.0),
                (8.0, -4.0),
            ]);
            line((0.0, 1.0), (0.0, 10.0));
            line((-4.0, -6.5), (4.0, -1.5));
        }
        Glyph::Folder => {
            poly(&[
                (-9.0, -6.0),
                (-2.0, -6.0),
                (0.0, -3.0),
                (9.0, -3.0),
                (9.0, 7.0),
                (-9.0, 7.0),
                (-9.0, -6.0),
            ]);
        }
        Glyph::Refresh => {
            p.add(egui::Shape::line(
                (0..24)
                    .map(|i| {
                        let t = 0.7 + i as f32 / 23.0 * 4.8;
                        point(t.cos() * 7.0, t.sin() * 7.0)
                    })
                    .collect(),
                stroke,
            ));
            poly(&[(2.0, -9.0), (6.0, -5.0), (9.0, -9.0)]);
        }
        Glyph::Info => {
            p.circle_stroke(c, 8.0 * s, stroke);
            line((0.0, -1.0), (0.0, 5.0));
            p.circle_filled(point(0.0, -5.0), 1.0 * s, color);
        }
        Glyph::Trash => {
            line((-8.0, -5.0), (8.0, -5.0));
            poly(&[(-5.0, -5.0), (-4.0, 9.0), (4.0, 9.0), (5.0, -5.0)]);
            poly(&[(-3.0, -5.0), (-3.0, -9.0), (3.0, -9.0), (3.0, -5.0)]);
        }
        Glyph::File => {
            poly(&[
                (-7.0, -10.0),
                (2.0, -10.0),
                (7.0, -5.0),
                (7.0, 10.0),
                (-7.0, 10.0),
                (-7.0, -10.0),
            ]);
            poly(&[(2.0, -10.0), (2.0, -5.0), (7.0, -5.0)]);
            line((-3.0, 0.0), (3.0, 0.0));
            line((-3.0, 4.0), (3.0, 4.0));
        }
        Glyph::Shield => {
            poly(&[
                (0.0, -10.0),
                (8.0, -6.0),
                (7.0, 3.0),
                (4.0, 7.0),
                (0.0, 10.0),
                (-4.0, 7.0),
                (-7.0, 3.0),
                (-8.0, -6.0),
                (0.0, -10.0),
            ]);
            poly(&[(-3.0, 0.0), (-1.0, 3.0), (4.0, -3.0)]);
        }
    }
}
pub fn icon(ui: &mut egui::Ui, g: Glyph, size: f32, color: Color32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    glyph(ui.painter(), r, g, color);
}
pub fn icon_button(ui: &mut egui::Ui, g: Glyph, tip: &str) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(r, 7.0, RAISED);
    }
    glyph(
        ui.painter(),
        r.shrink(7.0),
        g,
        if response.hovered() { TEXT } else { MUTED },
    );
    response.on_hover_text(tip)
}
pub fn primary(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            RichText::new(text)
                .font(medium(14.0))
                .color(Color32::from_rgb(21, 33, 52)),
        )
        .fill(BLUE)
        .min_size(Vec2::new(0.0, 38.0))
        .corner_radius(9),
    )
}
pub fn secondary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).font(medium(14.0)))
            .min_size(Vec2::new(0.0, 38.0))
            .corner_radius(9),
    )
}
pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).font(regular(11.5)).color(color));
        });
}
pub fn surface() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(16)
        .inner_margin(22)
}
pub fn header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).font(display(34.0)).color(TEXT));
    ui.add_space(2.0);
    ui.label(RichText::new(subtitle).font(regular(14.0)).color(MUTED));
    ui.add_space(23.0);
}
pub fn hero(p: &egui::Painter, rect: Rect, hover: bool) {
    let c = rect.center();
    let tint = if hover {
        BLUE
    } else {
        Color32::from_rgb(139, 164, 204)
    };
    for (off, label) in [(-78.0, "DEB"), (78.0, "SRC")] {
        let r = Rect::from_center_size(c + Vec2::new(off, 5.0), Vec2::new(60.0, 74.0));
        p.rect_filled(
            r.translate(Vec2::new(0.0, 6.0)),
            13.0,
            Color32::from_black_alpha(35),
        );
        p.rect_filled(r, 13.0, Color32::from_rgb(35, 40, 50));
        p.rect_stroke(
            r,
            13.0,
            Stroke::new(1.0_f32, Color32::from_rgb(58, 66, 81)),
            egui::StrokeKind::Inside,
        );
        glyph(
            p,
            Rect::from_center_size(r.center() - Vec2::new(0.0, 9.0), Vec2::splat(24.0)),
            Glyph::File,
            FAINT,
        );
        p.text(
            r.center() + Vec2::new(0.0, 20.0),
            egui::Align2::CENTER_CENTER,
            label,
            medium(10.0),
            MUTED,
        );
    }
    let r = Rect::from_center_size(c - Vec2::new(0.0, 6.0), Vec2::splat(94.0));
    for i in (1..=8).rev() {
        p.rect_filled(
            r.expand(i as f32 * 1.2).translate(Vec2::new(0.0, 4.0)),
            23.0,
            Color32::from_black_alpha(5),
        );
    }
    p.rect_filled(r, 22.0, Color32::from_rgb(43, 51, 66));
    p.rect_stroke(
        r,
        22.0,
        Stroke::new(1.0_f32, Color32::from_rgb(87, 108, 141)),
        egui::StrokeKind::Inside,
    );
    glyph(
        p,
        Rect::from_center_size(r.center(), Vec2::splat(43.0)),
        Glyph::Import,
        tint,
    );
    p.line_segment(
        [
            Pos2::new(r.left() + 24.0, r.top() + 1.0),
            Pos2::new(r.right() - 24.0, r.top() + 1.0),
        ],
        Stroke::new(1.0_f32, Color32::from_rgb(120, 139, 170)),
    );
}
