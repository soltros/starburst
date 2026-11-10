use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
pub const BG: Color32 = Color32::from_rgb(16, 19, 26);
pub const SURFACE: Color32 = Color32::from_rgb(24, 28, 38);
pub const BORDER: Color32 = Color32::from_rgb(44, 50, 64);
pub const TEXT: Color32 = Color32::from_rgb(236, 239, 245);
pub const MUTED: Color32 = Color32::from_rgb(155, 166, 185);
pub const ACCENT: Color32 = Color32::from_rgb(248, 184, 122);
#[derive(Clone, Copy)]
pub enum Icon {
    Download,
    Search,
    History,
    Settings,
    Server,
    Folder,
    Spark,
}
pub fn theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "Inter".into());
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = Color32::from_rgb(17, 21, 29);
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.hyperlink_color = ACCENT;
    style.visuals.selection.bg_fill = Color32::from_rgb(87, 64, 45);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, TEXT);
    for v in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        v.rounding = 8.0.into();
        v.fg_stroke = Stroke::new(1.4_f32, TEXT);
        v.bg_stroke = Stroke::new(1.0_f32, BORDER);
    }
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(35, 41, 54);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(35, 41, 54);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(47, 55, 71);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(47, 55, 71);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(86, 98, 120));
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(73, 58, 45);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(73, 58, 45);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 11.0);
    style.spacing.interact_size.y = 36.0;
    style.spacing.scroll.bar_width = 5.0;
    for (kind, size) in [
        (egui::TextStyle::Body, 15.0),
        (egui::TextStyle::Button, 14.0),
        (egui::TextStyle::Small, 13.0),
        (egui::TextStyle::Heading, 20.0),
    ] {
        style.text_styles.insert(kind, FontId::proportional(size));
    }
    ctx.set_style(style);
}
pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let p = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + x * rect.width() / 24.0,
            rect.top() + y * rect.height() / 24.0,
        )
    };
    let stroke = Stroke::new((rect.width() / 14.0).max(1.4), color);
    let line = |points: &[(f32, f32)]| {
        painter.add(egui::Shape::line(
            points.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    match icon {
        Icon::Download => {
            line(&[(12., 3.), (12., 15.)]);
            line(&[(7., 10.), (12., 15.), (17., 10.)]);
            line(&[(4., 16.), (4., 21.), (20., 21.), (20., 16.)]);
        }
        Icon::Search => {
            painter.circle_stroke(p(10., 10.), rect.width() * 0.29, stroke);
            line(&[(15., 15.), (21., 21.)]);
        }
        Icon::History => {
            painter.circle_stroke(p(12., 12.), rect.width() * 0.37, stroke);
            line(&[(12., 6.), (12., 12.), (16., 14.)]);
        }
        Icon::Settings => {
            line(&[(3., 6.), (21., 6.)]);
            line(&[(3., 12.), (21., 12.)]);
            line(&[(3., 18.), (21., 18.)]);
            for (x, y) in [(8., 6.), (16., 12.), (9., 18.)] {
                painter.circle_filled(p(x, y), rect.width() * 0.11, SURFACE);
                painter.circle_stroke(p(x, y), rect.width() * 0.11, stroke);
            }
        }
        Icon::Server => {
            for y in [4., 14.] {
                painter.rect_stroke(Rect::from_min_max(p(3., y), p(21., y + 6.)), 2.0, stroke);
                painter.circle_filled(p(7., y + 3.), rect.width() * 0.045, color);
                line(&[(12., y + 3.), (17., y + 3.)]);
            }
        }
        Icon::Folder => {
            line(&[
                (3., 7.),
                (3., 20.),
                (21., 20.),
                (21., 7.),
                (12., 7.),
                (10., 4.),
                (3., 4.),
                (3., 7.),
            ]);
        }
        Icon::Spark => {
            painter.add(egui::Shape::convex_polygon(
                [
                    (12., 1.),
                    (14., 8.),
                    (20., 4.),
                    (16., 10.),
                    (23., 12.),
                    (16., 14.),
                    (20., 20.),
                    (14., 16.),
                    (12., 23.),
                    (10., 16.),
                    (4., 20.),
                    (8., 14.),
                    (1., 12.),
                    (8., 10.),
                    (4., 4.),
                    (10., 8.),
                ]
                .iter()
                .map(|&(x, y)| p(x, y))
                .collect(),
                color,
                Stroke::NONE,
            ));
        }
    }
}
pub fn icon(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon(ui.painter(), rect, icon, color);
}
pub fn nav(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.0), Sense::click());
    let fill = if selected {
        Color32::from_rgb(57, 45, 37)
    } else if response.hovered() {
        Color32::from_rgb(31, 37, 49)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 9.0, fill);
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_size(
                rect.left_center() - egui::vec2(0.0, 10.0),
                egui::vec2(3.0, 20.0),
            ),
            2.0,
            ACCENT,
        );
    }
    let color = if selected { ACCENT } else { MUTED };
    paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.left_center() + egui::vec2(26., 0.), Vec2::splat(19.)),
        icon,
        color,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(48., 0.),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.),
        if selected { TEXT } else { MUTED },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
pub fn primary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(Color32::from_rgb(35, 26, 20)))
            .fill(ACCENT)
            .stroke(Stroke::NONE)
            .min_size(egui::vec2(0., 38.)),
    )
}
pub fn section(ui: &mut egui::Ui, icon_kind: Icon, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        icon(ui, icon_kind, 22., ACCENT);
        ui.label(RichText::new(title).size(18.).strong());
    });
    ui.label(RichText::new(subtitle).color(MUTED));
    ui.add_space(6.);
}
pub fn empty(ui: &mut egui::Ui, kind: Icon, title: &str, description: &str, action: &str) -> bool {
    let mut clicked = false;
    super::card(ui, |ui| {
        ui.add_space(8.);
        icon(ui, kind, 30., ACCENT);
        ui.add_space(10.);
        ui.label(RichText::new(title).size(19.).strong());
        ui.add_sized(
            [ui.available_width(), 38.0],
            egui::Label::new(RichText::new(description).color(MUTED))
                .wrap()
                .halign(egui::Align::LEFT),
        );
        ui.add_space(12.);
        clicked = primary(ui, action).clicked();
        ui.add_space(8.);
    });
    clicked
}

/// Keep release identifiers for downloading; present the title separately from encoding details.
pub fn release_title(raw: &str) -> (String, String) {
    let tokens: Vec<&str> = raw
        .split(['.', '_', ' '])
        .filter(|s| !s.is_empty())
        .collect();
    let quality = tokens.iter().position(|s| {
        ["480p", "576p", "720p", "1080p", "1080i", "2160p", "4320p"]
            .iter()
            .any(|q| s.eq_ignore_ascii_case(q))
    });
    match quality {
        Some(split) if split > 0 => (tokens[..split].join(" "), {
            let offset = raw
                .to_ascii_lowercase()
                .find(&tokens[split].to_ascii_lowercase())
                .unwrap_or(0);
            let chars: Vec<char> = raw[offset..].chars().collect();
            chars
                .iter()
                .enumerate()
                .map(|(i, &c)| {
                    if c == '_'
                        || (c == '.'
                            && !(i > 0
                                && i + 1 < chars.len()
                                && chars[i - 1].is_ascii_digit()
                                && chars[i + 1].is_ascii_digit()))
                    {
                        ' '
                    } else {
                        c
                    }
                })
                .collect::<String>()
        }),
        _ => (tokens.join(" "), String::new()),
    }
}
#[cfg(test)]
mod title_tests {
    use super::*;
    #[test]
    fn separates_technical_details_without_losing_episode() {
        let (title, details) =
            release_title("Star.Wars.The.Bad.Batch.S01E03.Replacements.2160p.WEB-DL.H265-SPWEB");
        assert_eq!(title, "Star Wars The Bad Batch S01E03 Replacements");
        assert_eq!(details, "2160p WEB-DL H265-SPWEB");
    }
}
