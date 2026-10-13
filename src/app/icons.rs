//! Tool bar icons: Phosphor icons (MIT, see README), drawn white once into
//! textures and tinted to the theme when painted.

use std::collections::HashMap;

use super::*;

const ICONS: &[(&str, &str)] = &[
    ("new", include_str!("../../assets/icons/file-plus.svg")),
    ("open", include_str!("../../assets/icons/folder-open.svg")),
    ("save", include_str!("../../assets/icons/floppy-disk.svg")),
    ("usb", include_str!("../../assets/icons/usb.svg")),
    ("test", include_str!("../../assets/icons/test-tube.svg")),
    ("print", include_str!("../../assets/icons/printer.svg")),
    ("undo", include_str!("../../assets/icons/arrow-u-up-left.svg")),
    ("redo", include_str!("../../assets/icons/arrow-u-up-right.svg")),
    ("turn-left", include_str!("../../assets/icons/arrow-counter-clockwise.svg")),
    ("turn-right", include_str!("../../assets/icons/arrow-clockwise.svg")),
    ("mirror", include_str!("../../assets/icons/flip-horizontal.svg")),
    ("center", include_str!("../../assets/icons/crosshair-simple.svg")),
    ("pin", include_str!("../../assets/icons/push-pin.svg")),
    ("fit", include_str!("../../assets/icons/frame-corners.svg")),
    ("zoom", include_str!("../../assets/icons/magnifying-glass-plus.svg")),
    ("play", include_str!("../../assets/icons/play.svg")),
    ("pause", include_str!("../../assets/icons/pause.svg")),
    ("needle", include_str!("../../assets/icons/needle.svg")),
    ("star", include_str!("../../assets/icons/star.svg")),
    ("star-fill", include_str!("../../assets/icons/star-fill.svg")),
    ("cursor", include_str!("../../assets/icons/cursor.svg")),
    ("bezier", include_str!("../../assets/icons/bezier-curve.svg")),
    ("pen", include_str!("../../assets/icons/pen-nib.svg")),
    ("rect", include_str!("../../assets/icons/rectangle.svg")),
    ("ellipse", include_str!("../../assets/icons/circle.svg")),
    ("polygon", include_str!("../../assets/icons/hexagon.svg")),
    ("line", include_str!("../../assets/icons/line-segment.svg")),
    ("bucket", include_str!("../../assets/icons/paint-bucket.svg")),
    ("angle", include_str!("../../assets/icons/angle.svg")),
    ("start", include_str!("../../assets/icons/flag-pennant.svg")),
];

/// Pixels the icons are drawn at; painted smaller, so they stay sharp on
/// scaled screens.
const PX: u32 = 48;

/// A one-color SVG drawn white, `px` square, to be tinted when painted.
pub(super) fn render(svg: &str, px: u32) -> Option<egui::ColorImage> {
    let svg = svg.replace("currentColor", "#ffffff");
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let mut pm = resvg::tiny_skia::Pixmap::new(px, px)?;
    let s = px as f32 / tree.size().width().max(tree.size().height());
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(s, s), &mut pm.as_mut());
    // tiny-skia keeps premultiplied alpha, which is what egui takes.
    let pixels = pm.pixels().iter().map(|p| Color32::from_rgba_premultiplied(p.red(), p.green(), p.blue(), p.alpha())).collect();
    Some(egui::ColorImage::new([px as usize, px as usize], pixels))
}

/// The icon's texture, made the first time it is asked for.
pub(super) fn texture(ctx: &egui::Context, name: &'static str) -> Option<egui::TextureHandle> {
    let id = egui::Id::new("hoop-icons");
    if let Some(t) = ctx.data(|d| d.get_temp::<HashMap<&'static str, egui::TextureHandle>>(id)).and_then(|m| m.get(name).cloned()) {
        return Some(t);
    }
    let svg = ICONS.iter().find(|i| i.0 == name)?.1;
    let tex = ctx.load_texture(format!("icon:{name}"), render(svg, PX)?, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.get_temp_mut_or_default::<HashMap<&'static str, egui::TextureHandle>>(id).insert(name, tex.clone()));
    Some(tex)
}

/// Paints an icon into `rect`, tinted.
pub(super) fn paint(ui: &egui::Ui, name: &'static str, rect: egui::Rect, tint: Color32) {
    if let Some(t) = texture(ui.ctx(), name) {
        ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
    }
}

/// A flat icon button for the tool bar: a frame on hover, the accent when on.
pub(super) fn button(ui: &mut egui::Ui, enabled: bool, on: bool, name: &'static str, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 28.0), if enabled { Sense::click() } else { Sense::hover() });
    let v = ui.visuals();
    if enabled && (resp.hovered() || on) {
        let fill = if on { v.selection.bg_fill.gamma_multiply(0.5) } else { v.widgets.hovered.weak_bg_fill };
        ui.painter().rect_filled(rect, 5, fill);
    }
    let tint = if !enabled {
        v.text_color().gamma_multiply(0.3)
    } else if resp.hovered() {
        v.strong_text_color()
    } else {
        v.text_color()
    };
    paint(ui, name, egui::Rect::from_center_size(rect.center(), vec2(18.0, 18.0)), tint);
    let resp = resp.on_hover_text(tip);
    enabled && resp.clicked()
}
