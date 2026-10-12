//! Small widgets shared by the panels: cards, buttons, rows with a slider
//! and a value box, thread pickers, and the interface font.

use super::*;

pub(super) fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Text color that reads on top of `c`.
pub(super) fn contrast(c: Color32) -> Color32 {
    if canvas::luminance([c.r(), c.g(), c.b()]) > 0.55 { Color32::from_rgb(0x14, 0x12, 0x13) } else { Color32::WHITE }
}

/// A rounded section with a small heading; every panel section uses one.
pub(super) fn card(ui: &mut egui::Ui, title: &str, accent: Color32, add: impl FnOnce(&mut egui::Ui)) {
    let v = ui.visuals();
    // A Qt-style group box: a thin frame with its title on top.
    let fill = v.panel_fill.lerp_to_gamma(v.text_color(), 0.025);
    let stroke = egui::Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color);
    ui.label(RichText::new(title).size(12.5).strong().color(v.text_color()));
    ui.add_space(-2.0);
    egui::Frame::new().fill(fill).stroke(stroke).corner_radius(4).inner_margin(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    let _ = accent;
    ui.add_space(8.0);
}

pub(super) fn primary(text: &str, accent: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(contrast(accent)).strong()).fill(accent)
}

/// Equal-width options in one row, the chosen one filled with the accent.
pub(super) fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)], accent: Color32) {
    let track = ui.visuals().extreme_bg_color;
    egui::Frame::new().fill(track).corner_radius(8).inner_margin(3).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let w = (ui.available_width() - 4.0 * (options.len() - 1) as f32) / options.len() as f32;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (v, name) in options {
                let on = *value == *v;
                let text = RichText::new(*name).color(if on { contrast(accent) } else { ui.visuals().text_color() });
                let b = egui::Button::new(text)
                    .fill(if on { accent } else { Color32::TRANSPARENT })
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(6)
                    .min_size(vec2(w, 26.0));
                if ui.add(b).clicked() {
                    *value = *v;
                }
            }
        });
    });
}

/// Like `slider_row`, for whole numbers.
pub(super) fn int_row<T: egui::emath::Numeric>(ui: &mut egui::Ui, label: &str, v: &mut T, range: std::ops::RangeInclusive<T>, hover: &str) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(62.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(62.0);
            ui.label(label);
        });
        ui.spacing_mut().slider_width = (ui.available_width() - 86.0).max(40.0);
        ui.add(egui::Slider::new(v, range.clone()).show_value(false)).on_hover_text(hover);
        ui.add_sized([78.0, 22.0], egui::DragValue::new(v).range(range).speed(0.05));
    });
}

/// Label, slider and an editable value, lined up the same in every row.
pub(super) fn slider_row(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str, decimals: usize) -> egui::Response {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(62.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(62.0);
            ui.label(label);
        });
        ui.spacing_mut().slider_width = (ui.available_width() - 86.0).max(40.0);
        let r = ui.add(egui::Slider::new(v, range.clone()).show_value(false));
        let d = ui.add_sized([78.0, 22.0], egui::DragValue::new(v).range(range).suffix(suffix).fixed_decimals(decimals).speed(0.1));
        r | d
    })
    .inner
}

#[allow(clippy::too_many_arguments)]
pub(super) fn hoop_tile(ui: &mut egui::Ui, w: f32, i: usize, h: &hoops::Hoop, fits: bool, selected: bool, accent: Color32, muted: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 58.0), Sense::click());
    let v = ui.visuals();
    let fill = if selected {
        accent.gamma_multiply(0.2)
    } else if resp.hovered() {
        v.widgets.hovered.weak_bg_fill
    } else {
        v.extreme_bg_color
    };
    let stroke = if selected { egui::Stroke::new(1.5, accent) } else { egui::Stroke::NONE };
    let p = ui.painter();
    p.rect(rect, 8, fill, stroke, egui::StrokeKind::Inside);
    let text = v.text_color();
    let x = rect.left() + 10.0;
    p.text(egui::pos2(x, rect.top() + 9.0), egui::Align2::LEFT_TOP, h.name, egui::FontId::proportional(15.0), text);
    p.text(egui::pos2(rect.right() - 10.0, rect.top() + 10.0), egui::Align2::RIGHT_TOP, (i + 1).to_string(), egui::FontId::monospace(11.0), muted);
    p.text(egui::pos2(x, rect.top() + 28.0), egui::Align2::LEFT_TOP, t!("{w} × {h} mm", w = h.w, h = h.h), egui::FontId::monospace(12.0), muted);
    let (note, c) = if fits { (crate::i18n::tr(h.note), muted) } else { (t!("Too small"), BAD) };
    p.text(egui::pos2(x, rect.top() + 43.0), egui::Align2::LEFT_TOP, note, egui::FontId::proportional(11.0), c);
    resp.on_hover_text(t!("Hoop {name} · key {v1}", name = h.name, v1 = i + 1))
}

/// A wrapping row of named style buttons; returns the one clicked.
pub(super) fn preset_chips(ui: &mut egui::Ui, presets: &[(&'static str, &'static str)]) -> Option<usize> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (i, (name, about)) in presets.iter().enumerate() {
            if ui.add(egui::Button::new(crate::i18n::tr(name)).corner_radius(12)).on_hover_text(crate::i18n::tr(about)).clicked() {
                picked = Some(i);
            }
        }
    });
    picked
}

/// A small button naming a Janome thread that opens the chart.
pub(super) fn thread_menu(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug + Copy, value: &mut i32) {
    let current = formats::jef_thread(*value, 0);
    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
    ui.painter().circle(r.center(), 7.0, rgb(current.color), egui::Stroke::new(1.0, Color32::from_black_alpha(90)));
    ui.menu_button(format!("{} {}", current.name, current.code), |ui| {
        egui::Grid::new(("chart", ui.id().with(id))).spacing([4.0, 4.0]).show(ui, |ui| {
            for (n, (i, t)) in formats::jef_palette().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                let stroke =
                    if i == *value { egui::Stroke::new(2.5, ui.visuals().selection.bg_fill) } else { egui::Stroke::new(1.0, Color32::from_black_alpha(80)) };
                ui.painter().rect(r, 5, rgb(t.color), stroke, egui::StrokeKind::Inside);
                if resp.on_hover_text(format!("{} {}", t.name, t.code)).clicked() {
                    *value = i;
                    ui.close();
                }
                if n % 9 == 8 {
                    ui.end_row();
                }
            }
        });
    });
}

/// The chosen Janome thread as a full-width button that opens the chart.
pub(super) fn jef_picker(ui: &mut egui::Ui, id: &str, value: &mut i32) {
    jef_picker_labeled(ui, id, t!("Thread"), value);
}

pub(super) fn jef_picker_labeled(ui: &mut egui::Ui, id: &str, label: &str, value: &mut i32) {
    let current = formats::jef_thread(*value, 0);
    let row = vec2(ui.available_width(), 26.0);
    ui.allocate_ui_with_layout(row, egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.allocate_ui_with_layout(vec2(62.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(62.0);
            ui.label(label);
        });
        let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
        ui.painter().circle(r.center(), 9.0, rgb(current.color), egui::Stroke::new(1.0, Color32::from_black_alpha(90)));
        ui.menu_button(format!("{}  {}", current.name, current.code), |ui| {
            egui::Grid::new(("chart", id)).spacing([4.0, 4.0]).show(ui, |ui| {
                for (n, (i, t)) in formats::jef_palette().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                    let stroke = if i == *value {
                        egui::Stroke::new(2.5, ui.visuals().selection.bg_fill)
                    } else {
                        egui::Stroke::new(1.0, Color32::from_black_alpha(80))
                    };
                    ui.painter().rect(r, 5, rgb(t.color), stroke, egui::StrokeKind::Inside);
                    if resp.on_hover_text(format!("{} {}", t.name, t.code)).clicked() {
                        *value = i;
                        ui.close();
                    }
                    if n % 9 == 8 {
                        ui.end_row();
                    }
                }
            });
        });
    });
}

/// Uses the Omarchy monospace font for the interface, like Omarchy's own apps.
pub(super) fn load_ui_font(ctx: &egui::Context, theme_font_everywhere: bool) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |s| {
            use egui::{FontId, TextStyle};
            s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
            s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
            s.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.5));
            s.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
            s.text_styles.insert(TextStyle::Heading, FontId::proportional(20.0));
            s.spacing.item_spacing = vec2(8.0, 6.0);
            s.spacing.button_padding = vec2(10.0, 5.0);
        });
    }
    let run = |cmd: &str, args: &[&str]| {
        std::process::Command::new(cmd).args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    // IBM Plex Sans (OFL, built in) for the interface; the theme's font,
    // usually monospace, for numbers. "ui_font = theme" uses the theme's
    // font everywhere.
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("plex".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../../assets/ui/IBMPlexSans-Regular.ttf"))));
    let theme_font =
        run("omarchy", &["font", "current"]).and_then(|family| run("fc-match", &["-f", "%{file}", &family])).and_then(|file| std::fs::read(file).ok());
    if let Some(bytes) = theme_font {
        fonts.font_data.insert("omarchy".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "omarchy".into());
    }
    let sans = !(theme_font_everywhere && fonts.font_data.contains_key("omarchy"));
    let first = if sans { "plex" } else { "omarchy" };
    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, first.into());
    ctx.set_fonts(fonts);
}
