//! Style cards shared by Lettering and Artwork: fabric presets, the fill
//! style tiles and their settings, and the edge style.

use super::*;

/// Fill style picker: tiles with a small drawing of each pattern.
pub(super) fn fill_tiles(ui: &mut egui::Ui, value: &mut FillKind, accent: Color32) {
    use egui::{Pos2, Shape, Stroke, pos2};
    const STYLES: [(FillKind, &str); 15] = [
        (FillKind::Auto, n!("Auto")),
        (FillKind::Column, n!("Column")),
        (FillKind::Tatami, n!("Tatami")),
        (FillKind::Satin, n!("Satin")),
        (FillKind::Contour, n!("Contour")),
        (FillKind::Wave, n!("Wave")),
        (FillKind::Zigzag, n!("Zigzag")),
        (FillKind::Crosshatch, n!("Hatch")),
        (FillKind::Gradient, n!("Gradient")),
        (FillKind::Open, n!("Open")),
        (FillKind::Stipple, n!("Stipple")),
        (FillKind::Cross, n!("Cross-stitch")),
        (FillKind::None, n!("None")),
        (FillKind::None, ""),
        (FillKind::None, ""),
    ];
    let w = (ui.available_width() - 18.0) / 4.0;
    for row in STYLES.chunks(4) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for &(kind, name) in row {
                if name.is_empty() {
                    continue;
                }
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 54.0), Sense::click());
                let v = ui.visuals();
                let on = *value == kind;
                let fill = if on {
                    accent.gamma_multiply(0.2)
                } else if resp.hovered() {
                    v.widgets.hovered.weak_bg_fill
                } else {
                    v.extreme_bg_color
                };
                let p = ui.painter();
                p.rect(rect, 8, fill, if on { Stroke::new(1.5, accent) } else { Stroke::NONE }, egui::StrokeKind::Inside);
                let icon = egui::Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(w - 12.0, 26.0));
                let ink = if on { accent } else { v.text_color().gamma_multiply(0.75) };
                let p = p.with_clip_rect(icon);
                let line = |a: Pos2, b: Pos2, width: f32| p.line_segment([a, b], Stroke::new(width, ink));
                let (l, r, t, bt) = (icon.left(), icon.right(), icon.top(), icon.bottom());
                match kind {
                    FillKind::Tatami | FillKind::Satin => {
                        let step = if kind == FillKind::Satin { 2.5 } else { 4.0 };
                        let mut x = l - 40.0;
                        let mut k = 0;
                        while x < r {
                            if kind == FillKind::Tatami {
                                // Short stitches with staggered needle points.
                                let mut y = t - 10.0 + (k % 3) as f32 * 3.0;
                                while y < bt {
                                    line(pos2(x + (y - t), y), pos2(x + (y - t) + 6.0, y + 6.0), 1.2);
                                    y += 8.0;
                                }
                            } else {
                                line(pos2(x, t), pos2(x + 30.0, bt), 1.2);
                            }
                            x += step;
                            k += 1;
                        }
                    }
                    FillKind::Wave | FillKind::Zigzag | FillKind::Open | FillKind::Gradient => {
                        let mut y = t + 2.0;
                        let mut gap = 4.0;
                        while y < bt + 4.0 {
                            let pts: Vec<Pos2> = (0..=40)
                                .map(|i| {
                                    let x = l + (r - l) * i as f32 / 40.0;
                                    let u = (x - l) / 16.0;
                                    let dy = match kind {
                                        FillKind::Wave => 2.5 * (u * std::f32::consts::TAU).sin(),
                                        FillKind::Zigzag => 2.5 * (4.0 * (u - (u + 0.5).floor()).abs() - 1.0),
                                        _ => 0.0,
                                    };
                                    pos2(x, y + dy)
                                })
                                .collect();
                            p.add(Shape::line(pts, Stroke::new(1.2, ink)));
                            y += match kind {
                                FillKind::Open => 7.0,
                                FillKind::Gradient => {
                                    gap += 1.2;
                                    gap - 1.2
                                }
                                _ => 4.0,
                            };
                        }
                    }
                    FillKind::Crosshatch => {
                        let mut x = l - 40.0;
                        while x < r {
                            line(pos2(x, t), pos2(x + 30.0, bt), 1.0);
                            line(pos2(x + 30.0, t), pos2(x, bt), 1.0);
                            x += 7.0;
                        }
                    }
                    FillKind::None => {
                        line(pos2(l + 6.0, bt - 4.0), pos2(r - 6.0, t + 4.0), 1.2);
                    }
                    FillKind::Stipple => {
                        // A wandering line.
                        let pts: Vec<Pos2> = (0..=120)
                            .map(|i| {
                                let u = i as f32 / 120.0;
                                let x = l + (r - l) * u;
                                let y = (t + bt) / 2.0 + (bt - t) * 0.42 * ((u * 23.0).sin() * 0.6 + (u * 9.0 + 1.0).cos() * 0.4);
                                pos2(x + 4.0 * (u * 40.0).sin(), y)
                            })
                            .collect();
                        p.add(Shape::line(pts, Stroke::new(1.1, ink)));
                    }
                    FillKind::Cross => {
                        let step = 7.0;
                        let mut y = t + 1.0;
                        while y + step <= bt {
                            let mut x = l + 2.0;
                            while x + step <= r {
                                line(pos2(x, y), pos2(x + step, y + step), 1.0);
                                line(pos2(x + step, y), pos2(x, y + step), 1.0);
                                x += step + 1.0;
                            }
                            y += step + 1.0;
                        }
                    }
                    FillKind::Column | FillKind::Auto => {
                        // A curved band sewn with stitches square to it; Auto
                        // adds a patch of tatami for the wide parts.
                        let right = if kind == FillKind::Auto { l + (r - l) * 0.55 } else { r };
                        let c = pos2((l + right) / 2.0, bt + 26.0);
                        let (r0, r1) = (30.0, 44.0);
                        let span = ((right - l) / 2.0 / r1).min(1.2);
                        let n = 26;
                        for k in 0..=n {
                            let a = -std::f32::consts::FRAC_PI_2 - span + 2.0 * span * k as f32 / n as f32;
                            let (cs, sn) = (a.cos(), a.sin());
                            line(pos2(c.x + cs * r0, c.y + sn * r0), pos2(c.x + cs * r1, c.y + sn * r1), 1.1);
                        }
                        if kind == FillKind::Auto {
                            let mut x = right + 4.0;
                            let mut k = 0;
                            while x < r {
                                let mut y = t - 10.0 + (k % 3) as f32 * 3.0;
                                while y < bt {
                                    line(pos2(x, y), pos2(x + 4.0, y + 4.0), 1.1);
                                    y += 7.0;
                                }
                                x += 4.0;
                                k += 1;
                            }
                        }
                    }
                    FillKind::Contour => {
                        let c = icon.center();
                        for k in 0..6 {
                            let inset = k as f32 * 3.5;
                            let rr = egui::Rect::from_center_size(c, icon.size() - vec2(2.0 * inset, 2.0 * inset));
                            if rr.width() > 2.0 && rr.height() > 2.0 {
                                p.rect_stroke(rr, 8.0 - inset.min(7.0), Stroke::new(1.1, ink), egui::StrokeKind::Middle);
                            }
                        }
                    }
                }
                ui.painter().text(
                    pos2(rect.center().x, rect.bottom() - 11.0),
                    egui::Align2::CENTER_CENTER,
                    crate::i18n::tr(name),
                    egui::FontId::proportional(11.5),
                    if on { accent } else { v.text_color() },
                );
                if resp.clicked() {
                    *value = kind;
                }
            }
        });
    }
}

/// Fabrics and how they change the stitching: (name, extra row spacing in
/// mm, pull compensation, stitch length, advice shown when picked).
/// Fabric presets, like a digitizing program's fabric assistant: name,
/// change to the style's row spacing, pull compensation, stitch length,
/// underlay, and advice.
pub(super) const FABRICS_SEW: [(&str, f32, f32, f32, lettering::Underlay, &str); 9] = [
    (n!("Woven"), 0.0, 0.2, 3.5, lettering::Underlay::Auto, n!("Woven cotton: tear-away stabilizer is enough.")),
    (n!("Knit"), 0.05, 0.35, 3.0, lettering::Underlay::Tatami, n!("T-shirts and knits: use cut-away stabilizer and hoop without stretching.")),
    (n!("Denim"), 0.0, 0.15, 4.0, lettering::Underlay::Auto, n!("Denim and canvas: tear-away stabilizer, a size 90 needle.")),
    (
        n!("Towel"),
        -0.05,
        0.3,
        3.0,
        lettering::Underlay::Double,
        n!("Towels and fleece: lay water-soluble topping on top so stitches don't sink into the pile."),
    ),
    (n!("Fleece"), -0.05, 0.35, 3.0, lettering::Underlay::Double, n!("Fleece: cut-away stabilizer below, water-soluble topping on top.")),
    (n!("Felt"), 0.05, 0.1, 4.0, lettering::Underlay::Edge, n!("Felt barely moves; light underlay keeps it flat. No stabilizer needed for patches.")),
    (n!("Leather"), 0.1, 0.1, 4.0, lettering::Underlay::Edge, n!("Leather and vinyl: every hole stays, so fewer, longer stitches and a leather needle.")),
    (n!("Silk"), 0.05, 0.25, 2.5, lettering::Underlay::Edge, n!("Silk and thin fabric: a fine needle, light fills and a soft tear-away or wash-away.")),
    (n!("Cap"), 0.0, 0.3, 3.5, lettering::Underlay::Zigzag, n!("Caps and stiff canvas: sew from the center out; zigzag underlay helps on the curve.")),
];

/// Quick fabric presets for the fill. Returns the advice for a picked one.
pub(super) fn fabric_card(ui: &mut egui::Ui, s: &mut Settings, muted: Color32) -> Option<&'static str> {
    let mut tip = None;
    let accent = ui.visuals().selection.bg_fill;
    card_folded(ui, t!("Fabric"), accent, |ui| {
        ui.label(RichText::new(t!("Sets row spacing, pull, stitch length and underlay for the material.")).size(12.0).color(muted));
        let base = s.fill.default_rows_mm();
        ui.horizontal_wrapped(|ui| {
            for (name, rows, pull, len, under, advice) in FABRICS_SEW {
                let on = (s.density_mm - (base + rows)).abs() < 0.001
                    && (s.pull_mm - pull).abs() < 0.001
                    && (s.stitch_mm - len).abs() < 0.001
                    && s.underlay_kind == under;
                if ui.selectable_label(on, crate::i18n::tr(name)).on_hover_text(crate::i18n::tr(advice)).clicked() {
                    s.density_mm = base + rows;
                    s.pull_mm = pull;
                    s.stitch_mm = len;
                    s.underlay = true;
                    s.underlay_kind = under;
                    tip = Some(crate::i18n::tr(advice));
                }
            }
        });
    });
    tip
}

/// Fill style tiles and the settings of the chosen style.
pub(super) fn fill_card(ui: &mut egui::Ui, s: &mut Settings, muted: Color32, accent: Color32, with_thread: bool) {
    card(ui, t!("Fill"), accent, |ui| {
        let before = s.fill;
        fill_tiles(ui, &mut s.fill, accent);
        if s.fill != before {
            s.density_mm = s.fill.default_rows_mm();
            // An outline under an open fill would show, so it starts off there.
            s.underlay = !matches!(s.fill, FillKind::Open | FillKind::Crosshatch);
        }
        if s.fill == FillKind::None {
            return;
        }
        ui.add_space(4.0);
        if with_thread {
            jef_picker(ui, "fill", &mut s.fill_thread);
        }
        if !matches!(s.fill, FillKind::Contour | FillKind::Column | FillKind::Stipple | FillKind::Cross) {
            slider_row(ui, t!("Angle"), &mut s.fill_angle, 0.0..=180.0, "°", 0).on_hover_text(t!("Direction of the stitch rows"));
        }
        if matches!(s.fill, FillKind::Crosshatch | FillKind::Open | FillKind::Gradient) {
            slider_row(ui, t!("Rows"), &mut s.density_mm, 0.25..=3.0, " mm", 2).on_hover_text(t!("Distance between fill rows"));
        }
        if s.fill == FillKind::Stipple {
            slider_row(ui, t!("Spacing"), &mut s.density_mm, 1.0..=6.0, " mm", 1).on_hover_text(t!("Distance between the meandering lines"));
        }
        if s.fill == FillKind::Cross {
            slider_row(ui, t!("Cell"), &mut s.density_mm, 1.5..=6.0, " mm", 1).on_hover_text(t!("Size of each cross"));
        }
        if s.fill == FillKind::Gradient {
            slider_row(ui, t!("Fade"), &mut s.fade_mm, 0.5..=6.0, " mm", 1).on_hover_text(t!("Row spacing at the light end"));
        }
        if matches!(s.fill, FillKind::Wave | FillKind::Zigzag) {
            slider_row(ui, t!("Swing"), &mut s.wave_amp_mm, 0.5..=8.0, " mm", 1).on_hover_text(t!("How far the rows swing up and down"));
            slider_row(ui, t!("Length"), &mut s.wave_len_mm, 3.0..=40.0, " mm", 1).on_hover_text(t!("Distance from one wave to the next"));
        }
        let note = match s.fill {
            FillKind::Satin => t!("Satin looks best on strokes under 8 mm wide."),
            FillKind::Wave | FillKind::Zigzag => t!("The pattern shows in how the thread catches the light."),
            FillKind::Crosshatch | FillKind::Open => t!("Open fills let the fabric show through. Nice on felt and canvas."),
            FillKind::Gradient => t!("Rows thin out across each shape. Turn the angle to set the direction."),
            FillKind::Contour => t!("Rings follow the outline inward. Lovely on round shapes and bold letters."),
            FillKind::Column => t!("Satin that turns with the shape, like hand-digitized lettering and borders. For parts up to about 7 mm wide."),
            FillKind::Stipple => t!("A meandering line that never crosses itself. Quilting look; pair it with a triple outline."),
            FillKind::Cross => t!("Hand-stitched look: an X in every cell. Use bigger cells on thick fabric."),
            FillKind::Auto => t!("Narrow parts become turning satin columns, wide parts tatami at this angle."),
            _ => "",
        };
        if !note.is_empty() {
            ui.label(RichText::new(note).size(12.0).color(muted));
        }
        egui::CollapsingHeader::new(RichText::new(t!("More settings")).color(muted)).id_salt(("fill-more", with_thread)).show(ui, |ui| {
            if !matches!(s.fill, FillKind::Crosshatch | FillKind::Open | FillKind::Gradient) {
                slider_row(ui, t!("Rows"), &mut s.density_mm, 0.25..=3.0, " mm", 2).on_hover_text(t!("Distance between fill rows. Smaller is denser."));
            }
            if s.fill != FillKind::Satin {
                slider_row(ui, t!("Stitch"), &mut s.stitch_mm, 2.0..=6.0, " mm", 1).on_hover_text(t!("Stitch length along each row"));
            }
            slider_row(ui, t!("Pull"), &mut s.pull_mm, 0.0..=1.0, " mm", 2)
                .on_hover_text(t!("Stitches pull the fabric in, so rows are made this much longer at each end"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.underlay, t!("Underlay")).on_hover_text(t!("Stitches under the fill that hold the fabric and lift the top stitches"));
                ui.add_enabled_ui(s.underlay, |ui| {
                    egui::ComboBox::from_id_salt(("underlay", with_thread))
                        .selected_text(crate::i18n::tr(s.underlay_kind.name()))
                        .width(ui.available_width() - 8.0)
                        .show_ui(ui, |ui| {
                            for u in lettering::Underlay::ALL {
                                ui.selectable_value(&mut s.underlay_kind, u, crate::i18n::tr(u.name()));
                            }
                        });
                });
            });
        });
    });
}

pub(super) fn edge_card(ui: &mut egui::Ui, s: &mut Settings, accent: Color32, title: &str, with_thread: bool) {
    card(ui, title, accent, |ui| {
        segmented(
            ui,
            &mut s.edge,
            &[(EdgeKind::Satin, t!("Satin")), (EdgeKind::Running, t!("Run")), (EdgeKind::Triple, t!("Triple")), (EdgeKind::None, t!("None"))],
            accent,
        );
        segmented(ui, &mut s.edge, &[(EdgeKind::Blanket, t!("Blanket")), (EdgeKind::Scallop, t!("Scallop"))], accent);
        if matches!(s.edge, EdgeKind::Satin | EdgeKind::Blanket | EdgeKind::Scallop) {
            ui.add_space(4.0);
            slider_row(ui, t!("Width"), &mut s.edge_width_mm, 0.8..=5.0, " mm", 1);
        }
        if s.edge == EdgeKind::Blanket {
            ui.label(RichText::new(t!("The classic appliqué edge: prongs reach into the shape.")).size(12.0).color(ui.visuals().weak_text_color()));
        }
        if with_thread && s.edge != EdgeKind::None {
            jef_picker(ui, "edge", &mut s.edge_thread);
        }
    });
}
