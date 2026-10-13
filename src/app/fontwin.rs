//! Google Fonts window (Lettering → font list → Get more fonts, or Machine →
//! Google Fonts): the open-licensed Google Fonts families, each shown in its
//! own letters with how small it stitches, and one click to add a weight to
//! the font list. Also keeps the font list's previews and size estimates.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use egui::{RichText, vec2};

use super::*;
use crate::gfonts::{self, Family};
use crate::lettering::{FontData, FontSource};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Sort {
    /// Heavy strokes first: they stitch smallest.
    #[default]
    Fit,
    Popular,
    Newest,
    Name,
}

type Key = (String, u16);
/// An add (true) or remove of one weight, and where its answer comes.
type Job = (Key, bool, Receiver<Result<PathBuf, String>>);
/// A finished preview, with the `View::generation` it was made for.
type Done = (u64, Key, Result<(egui::ColorImage, Option<f32>), String>);

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Look {
    /// The letters as drawn.
    #[default]
    Letters,
    /// Run through the stitch engine with the Lettering style and threads.
    Stitched,
}

/// What previews show; workers read it when they start a preview.
#[derive(Clone, Default)]
struct View {
    generation: u64,
    look: Look,
    /// Shown instead of the family name when not empty.
    sample: String,
    style: lettering::Settings,
    fabric: [u8; 3],
    /// Preview height in pixels.
    px: f32,
}

impl View {
    fn same(&self, o: &View) -> bool {
        self.look == o.look && self.sample == o.sample && self.px == o.px && (self.look == Look::Letters || (self.style == o.style && self.fabric == o.fabric))
    }
}

/// The Lettering style a stitched preview uses: the user's fill, edge and
/// threads on one straight line of 15 mm capitals.
fn preview_style(s: &lettering::Settings) -> lettering::Settings {
    lettering::Settings {
        text: String::new(),
        font: 0,
        height_mm: 15.0,
        shape: lettering::TextShape::Straight,
        slant_deg: 0.0,
        patch: lettering::PatchShape::None,
        ..s.clone()
    }
}

enum Preview {
    Loading,
    Ready(egui::TextureHandle, Option<f32>),
    Failed,
}

/// Preview heights in points.
const PREVIEW_H: f32 = 30.0;
const STITCHED_H: f32 = 64.0;
const WORKERS: usize = 4;

pub(super) struct FontBrowser {
    pub open: bool,
    catalog: Option<Result<Vec<Family>, String>>,
    loading: Option<Receiver<Result<Vec<Family>, String>>>,
    filter: String,
    category: Option<&'static str>,
    sort: Sort,
    look: Look,
    sample: String,
    view: Arc<Mutex<View>>,
    focus_search: bool,
    /// Search, kind, order and look the list was last shown for.
    list_key: Option<(String, Option<&'static str>, u8, u8)>,
    /// The weight picked for each family.
    weight: HashMap<String, u16>,
    previews: HashMap<Key, Preview>,
    queue: Arc<Mutex<Vec<Key>>>,
    workers: Arc<AtomicUsize>,
    done_tx: Sender<Done>,
    done_rx: Receiver<Done>,
    /// An add or remove in progress.
    job: Option<Job>,
    message: Option<Result<String, String>>,
    /// Font list previews of bundled and added fonts, by name.
    thumbs: HashMap<String, Option<egui::TextureHandle>>,
    /// How small each font in the list stitches, by name.
    pub sizes: HashMap<String, Option<f32>>,
    sizing: Option<Receiver<(String, Option<f32>)>>,
}

impl Default for FontBrowser {
    fn default() -> Self {
        let (done_tx, done_rx) = channel();
        FontBrowser {
            open: false,
            catalog: None,
            loading: None,
            filter: String::new(),
            category: None,
            sort: Sort::default(),
            look: Look::default(),
            sample: String::new(),
            view: Default::default(),
            focus_search: true,
            list_key: None,
            weight: HashMap::new(),
            previews: HashMap::new(),
            queue: Default::default(),
            workers: Default::default(),
            done_tx,
            done_rx,
            job: None,
            message: None,
            thumbs: HashMap::new(),
            sizes: HashMap::new(),
            sizing: None,
        }
    }
}

impl FontBrowser {
    /// Opens on a search typed elsewhere (the font list), across all kinds.
    pub fn search(&mut self, query: &str) {
        self.show();
        self.filter = query.trim().to_string();
        self.category = None;
    }

    /// Opens the window with the cursor in its search box.
    pub fn show(&mut self) {
        self.open = true;
        self.focus_search = true;
    }
}

/// Loads previews from the queue, newest request first, until it is empty.
fn spawn_worker(fb: &FontBrowser, ctx: &egui::Context) {
    let (queue, workers, tx, ctx, view) = (fb.queue.clone(), fb.workers.clone(), fb.done_tx.clone(), ctx.clone(), fb.view.clone());
    workers.fetch_add(1, Ordering::SeqCst);
    std::thread::spawn(move || {
        loop {
            let Some(key) = queue.lock().unwrap().pop() else { break };
            let v = view.lock().unwrap().clone();
            let r = make_preview(&key, &v);
            if tx.send((v.generation, key, r)).is_err() {
                break;
            }
            ctx.request_repaint();
        }
        workers.fetch_sub(1, Ordering::SeqCst);
    });
}

fn make_preview(key: &Key, v: &View) -> Result<(egui::ColorImage, Option<f32>), String> {
    let path = gfonts::preview(&key.0, key.1, &v.sample)?;
    let font = FontSource { name: key.0.clone(), featured: false, group: None, data: FontData::File(path.clone(), 0) };
    let mm = lettering::smallest_height_mm(&font);
    let text = if v.sample.trim().is_empty() { key.0.clone() } else { v.sample.clone() };
    let failed = || t!("Google Fonts sent no font file.").to_string();
    let img = match v.look {
        Look::Letters => {
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            lettering::render_line(&data, &text, v.px).ok_or_else(failed)?
        }
        Look::Stitched => {
            let style = lettering::Settings { text, ..v.style.clone() };
            let pattern = lettering::generate(&font, &style)?;
            // 15 mm capitals fill about 60 % of the height.
            crate::library::draw_stitched(&pattern, v.px as u32, v.px * 0.6 / 150.0, v.fabric).ok_or_else(failed)?
        }
    };
    Ok((img, mm))
}

fn size_note(mm: Option<f32>) -> String {
    mm.map_or(String::new(), |mm| t!("stitches from {mm} mm", mm = mm))
}

impl HoopApp {
    /// A preview of a font in the list, made on first use: bundled and
    /// added fonts only, system fonts show their name in the UI font.
    pub(super) fn font_thumb(&mut self, ctx: &egui::Context, i: usize) -> Option<egui::TextureHandle> {
        let f = self.fonts.get(i)?;
        if !f.featured && !f.downloaded() {
            return None;
        }
        if let Some(t) = self.fontwin.thumbs.get(&f.name) {
            return t.clone();
        }
        let px = 22.0 * ctx.pixels_per_point();
        let tex = f
            .bytes()
            .ok()
            .and_then(|(b, _)| lettering::render_line(&b, &f.name, px))
            .map(|img| ctx.load_texture(format!("font-{}", f.name), img, Default::default()));
        self.fontwin.thumbs.insert(f.name.clone(), tex.clone());
        tex
    }

    /// Works out how small the bundled and added fonts stitch, and the
    /// current font, off the UI thread.
    pub(super) fn update_font_sizes(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.fontwin.sizing {
            for (name, mm) in rx.try_iter() {
                self.fontwin.sizes.insert(name, mm);
            }
        }
        let current = self.fonts.get(self.letter.font).map(|f| f.name.clone());
        let missing: Vec<FontSource> = self
            .fonts
            .iter()
            .filter(|f| (f.featured || f.downloaded() || Some(&f.name) == current.as_ref()) && !self.fontwin.sizes.contains_key(&f.name))
            .cloned()
            .collect();
        if missing.is_empty() {
            return;
        }
        for f in &missing {
            self.fontwin.sizes.insert(f.name.clone(), None);
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for f in missing {
                if tx.send((f.name.clone(), lettering::smallest_height_mm(&f))).is_err() {
                    return;
                }
            }
            ctx.request_repaint();
        });
        // Results of an earlier batch still arrive: keep both channels by
        // draining the old one first.
        if let Some(old) = self.fontwin.sizing.replace(rx) {
            for (name, mm) in old.try_iter() {
                self.fontwin.sizes.insert(name, mm);
            }
        }
    }

    /// Puts an added font in the list after the bundled and other added
    /// ones, keeping the chosen font chosen; returns its place.
    fn add_font(&mut self, font: FontSource) -> usize {
        if let Some(i) = self.fonts.iter().position(|f| f.name == font.name) {
            self.fonts[i] = font;
            return i;
        }
        let at =
            self.fonts.iter().position(|f| !f.featured && (!f.downloaded() || f.name.to_lowercase() > font.name.to_lowercase())).unwrap_or(self.fonts.len());
        self.fonts.insert(at, font);
        if self.letter.font >= at {
            self.letter.font += 1;
        }
        at
    }

    pub(super) fn fonts_window(&mut self, ctx: &egui::Context) {
        if !self.fontwin.open {
            return;
        }
        let fb = &mut self.fontwin;
        if fb.catalog.is_none() && fb.loading.is_none() {
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(gfonts::catalog());
                ctx.request_repaint();
            });
            fb.loading = Some(rx);
        }
        if let Some(rx) = &fb.loading
            && let Ok(r) = rx.try_recv()
        {
            fb.catalog = Some(r);
            fb.loading = None;
        }
        // A new look, sample or Lettering style makes all previews again.
        let wanted_view = View {
            generation: 0,
            look: fb.look,
            sample: fb.sample.trim().to_string(),
            style: preview_style(&self.letter),
            fabric: self.fabric,
            px: (if fb.look == Look::Stitched { STITCHED_H } else { PREVIEW_H } * ctx.pixels_per_point()).round(),
        };
        {
            let mut v = fb.view.lock().unwrap();
            if !v.same(&wanted_view) {
                *v = View { generation: v.generation + 1, ..wanted_view };
                fb.previews.clear();
                fb.queue.lock().unwrap().clear();
            }
        }
        let generation = fb.view.lock().unwrap().generation;
        for (made_for, key, r) in fb.done_rx.try_iter() {
            if made_for != generation {
                continue;
            }
            let p = match r {
                Ok((img, mm)) => Preview::Ready(ctx.load_texture(format!("gf-{}-{}", key.0, key.1), img, Default::default()), mm),
                Err(_) => Preview::Failed,
            };
            fb.previews.insert(key, p);
        }
        if let Some((key, adding, rx)) = &fb.job
            && let Ok(r) = rx.try_recv()
        {
            let (key, adding) = (key.clone(), *adding);
            self.fontwin.job = None;
            self.finish_font_job(key, adding, r);
        }

        let muted = self.muted();
        let accent = self.accent();
        let mut open = true;
        let mut start: Option<(Key, bool)> = None;
        let in_use = self.fonts.get(self.letter.font).and_then(|f| match &f.data {
            FontData::File(p, _) => Some(p.clone()),
            _ => None,
        });
        let built_in: Vec<String> = self.fonts.iter().filter(|f| f.featured).map(|f| f.name.clone()).collect();
        let lettering_text: String = self.letter.text.lines().next().unwrap_or("").chars().take(40).collect();
        let fb = &mut self.fontwin;
        egui::Window::new(t!("Google Fonts")).open(&mut open).collapsible(false).resizable(true).default_size(vec2(760.0, 760.0)).show(ctx, |ui| {
            let text_color = ui.visuals().text_color();
            ui.label(
                RichText::new(t!("Free fonts from Google Fonts (SIL Open Font License or Apache 2.0). Added fonts go to ~/.local/share/fonts/hoop with their license, and other programs can use them too."))
                    .size(12.0)
                    .color(muted),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let search = ui.add(egui::TextEdit::singleline(&mut fb.filter).hint_text(t!("Search fonts")).desired_width(220.0));
                if fb.focus_search {
                    search.request_focus();
                    fb.focus_search = false;
                }
                egui::ComboBox::from_id_salt("gf-sort")
                    .selected_text(match fb.sort {
                        Sort::Fit => t!("Best for stitching"),
                        Sort::Popular => t!("Most popular"),
                        Sort::Newest => t!("Newest"),
                        Sort::Name => t!("Name"),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut fb.sort, Sort::Fit, t!("Best for stitching"))
                            .on_hover_text(t!("Fonts with heavy strokes first: they stitch cleanly at smaller sizes"));
                        ui.selectable_value(&mut fb.sort, Sort::Popular, t!("Most popular"));
                        ui.selectable_value(&mut fb.sort, Sort::Newest, t!("Newest"));
                        ui.selectable_value(&mut fb.sort, Sort::Name, t!("Name"));
                    });
                if ui.button(t!("Open folder")).on_hover_text(t!("The folder added fonts are kept in")).clicked()
                    && let Some(d) = lettering::download_dir()
                {
                    let _ = std::fs::create_dir_all(&d);
                    let _ = std::process::Command::new("xdg-open").arg(d).spawn();
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(fb.category.is_none(), t!("All")).clicked() {
                    fb.category = None;
                }
                for c in gfonts::CATEGORIES {
                    if ui.selectable_label(fb.category == Some(c), gfonts::category_name(c)).clicked() {
                        fb.category = Some(c);
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(t!("Show"));
                ui.allocate_ui(vec2(200.0, 28.0), |ui| {
                    segmented(ui, &mut fb.look, &[(Look::Letters, t!("Letters")), (Look::Stitched, t!("Stitched"))], accent);
                });
                ui.add(egui::TextEdit::singleline(&mut fb.sample).hint_text(t!("Sample text (font names if empty)")).desired_width(200.0));
                if ui.button(t!("Use my text")).on_hover_text(t!("The first line of your Lettering text")).clicked() {
                    fb.sample = lettering_text.clone();
                }
            });
            if fb.look == Look::Stitched {
                ui.label(RichText::new(t!("Stitched with your Lettering fill, edge and threads at 15 mm. Change them in Lettering to compare.")).size(12.0).color(muted));
            }
            if let Some(m) = &fb.message {
                match m {
                    Ok(t) => ui.label(RichText::new(t).color(super::OK)),
                    Err(e) => ui.label(RichText::new(e).color(super::BAD)),
                };
            }
            ui.separator();
            let fams = match &fb.catalog {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(t!("Loading the Google Fonts list…")).color(muted));
                    });
                    return;
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(super::BAD));
                    if ui.button(t!("Try again")).clicked() {
                        fb.catalog = None;
                    }
                    return;
                }
                Some(Ok(f)) => f,
            };
            let mut shown: Vec<&Family> = fams.iter().filter(|f| fb.category.is_none_or(|c| f.category == c) && lettering::font_matches(&f.name, &fb.filter)).collect();
            match fb.sort {
                Sort::Fit => shown.sort_by(|a, b| b.thickness.total_cmp(&a.thickness).then(a.popularity.cmp(&b.popularity))),
                Sort::Popular => {}
                Sort::Newest => shown.sort_by(|a, b| b.added.cmp(&a.added)),
                Sort::Name => shown.sort_by_key(|f| f.name.to_lowercase()),
            }
            ui.label(RichText::new(tn!(shown.len(), "{n} family", "{n} families")).size(12.0).color(muted));
            let preview_h = if fb.look == Look::Stitched { STITCHED_H } else { PREVIEW_H };
            let row_h = preview_h + 26.0;
            let mut wanted: Vec<Key> = Vec::new();
            if shown.is_empty() {
                ui.label(RichText::new(t!("No family matches. Try fewer letters or another kind.")).color(muted));
            }
            // A new search, kind, order or look starts at the top; an
            // old scroll position could otherwise show an empty list.
            let list_key = (fb.filter.clone(), fb.category, fb.sort as u8, fb.look as u8);
            let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if fb.list_key.as_ref() != Some(&list_key) {
                scroll = scroll.vertical_scroll_offset(0.0);
                fb.list_key = Some(list_key);
            }
            scroll.show_rows(ui, row_h, shown.len(), |ui, range| {
                for f in &shown[range] {
                    let weight = *fb.weight.get(&f.name).unwrap_or(&f.default_weight());
                    let key = (f.name.clone(), weight);
                    let installed = gfonts::is_installed(&f.name, weight);
                    ui.allocate_ui(vec2(ui.available_width(), row_h), |ui| {
                        ui.horizontal(|ui| {
                            let left = (ui.available_width() - 230.0).max(120.0);
                            ui.vertical(|ui| {
                                ui.set_width(left);
                                let mut mm = None;
                                match fb.previews.get(&key) {
                                    Some(Preview::Ready(tex, est)) => {
                                        let size = tex.size_vec2() / ui.ctx().pixels_per_point();
                                        let size = if size.x > left { size * (left / size.x) } else { size };
                                        let tint = if fb.look == Look::Letters { text_color } else { egui::Color32::WHITE };
                                        ui.add(egui::Image::new((tex.id(), size)).tint(tint).corner_radius(4.0));
                                        mm = *est;
                                    }
                                    Some(Preview::Failed) => {
                                        ui.add_sized(vec2(left, preview_h), egui::Label::new(RichText::new(&f.name).size(18.0)));
                                    }
                                    _ => {
                                        if !fb.previews.contains_key(&key) {
                                            wanted.push(key.clone());
                                        }
                                        ui.add_sized(vec2(left, preview_h), egui::Label::new(RichText::new(&f.name).size(18.0).color(muted)));
                                    }
                                }
                                let mut info = format!("{} · {}", gfonts::category_name(&f.category), tn!(f.weights.len(), "{n} weight", "{n} weights"));
                                // The preview shows the sample, not the name.
                                let named = fb.look == Look::Letters && fb.sample.trim().is_empty();
                                if mm.is_some() {
                                    info += &format!(" · {}", size_note(mm));
                                }
                                ui.horizontal(|ui| {
                                    if !named {
                                        ui.label(RichText::new(&f.name).size(12.0).strong());
                                    }
                                    ui.label(RichText::new(info).size(11.0).color(muted));
                                });
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let busy = fb.job.is_some();
                                // Bundled fonts are the regular weight, but
                                // Cinzel Decorative is its Black.
                                let bundled_weight = if f.name == "Cinzel Decorative" { 900 } else { 400 };
                                if built_in.contains(&f.name) && weight == bundled_weight {
                                    ui.label(RichText::new(t!("Built into Hoop")).color(muted));
                                } else if installed {
                                    let path = gfonts::installed_path(&f.name, weight);
                                    let used = path.is_some() && path == in_use;
                                    if ui
                                        .add_enabled(!busy && !used, egui::Button::new(t!("Remove")))
                                        .on_disabled_hover_text(t!("It's the font in use. Pick another font first."))
                                        .clicked()
                                    {
                                        start = Some((key.clone(), false));
                                    }
                                    ui.label(RichText::new(t!("In the list")).color(super::OK));
                                } else if ui.add_enabled(!busy, egui::Button::new(RichText::new(t!("Add")).color(accent))).clicked() {
                                    start = Some((key.clone(), true));
                                }
                                if fb.job.as_ref().is_some_and(|j| j.0 == key) {
                                    ui.spinner();
                                }
                                let mut w = weight;
                                egui::ComboBox::from_id_salt(("gf-w", &f.name)).selected_text(gfonts::weight_name(w)).width(96.0).show_ui(ui, |ui| {
                                    for &x in &f.weights {
                                        let mut label = gfonts::weight_name(x).to_string();
                                        if gfonts::is_installed(&f.name, x) {
                                            label += " •";
                                        }
                                        ui.selectable_value(&mut w, x, label);
                                    }
                                });
                                if w != weight {
                                    fb.weight.insert(f.name.clone(), w);
                                }
                            });
                        });
                    });
                }
            });
            if !wanted.is_empty() {
                for k in &wanted {
                    fb.previews.insert(k.clone(), Preview::Loading);
                }
                let mut q = fb.queue.lock().unwrap();
                q.extend(wanted);
                // Rows scrolled past long ago aren't worth fetching.
                let len = q.len();
                if len > 60 {
                    for k in q.drain(..len - 60) {
                        fb.previews.remove(&k);
                    }
                }
            }
        });
        let fb = &mut self.fontwin;
        let queued = !fb.queue.lock().unwrap().is_empty();
        if queued {
            for _ in fb.workers.load(Ordering::SeqCst)..WORKERS {
                spawn_worker(fb, ctx);
            }
        }
        if let Some((key, adding)) = start {
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            let k = key.clone();
            std::thread::spawn(move || {
                let r = if adding {
                    gfonts::install(&k.0, k.1)
                } else {
                    gfonts::remove(&k.0, k.1).and_then(|_| gfonts::installed_path(&k.0, k.1).ok_or_else(|| t!("No home folder.").to_string()))
                };
                let _ = tx.send(r);
                ctx.request_repaint();
            });
            fb.message = None;
            fb.job = Some((key, adding, rx));
        }
        if !open {
            self.fontwin.open = false;
        }
    }

    fn finish_font_job(&mut self, key: Key, adding: bool, r: Result<PathBuf, String>) {
        let label = format!("{} {}", key.0, gfonts::weight_name(key.1));
        let message = match r {
            Ok(path) if adding => match lettering::font_file(&path) {
                Some(font) => {
                    let name = font.name.clone();
                    let at = self.add_font(font);
                    if self.mode == Mode::Lettering {
                        self.letter.font = at;
                        Ok(t!("{v0} is in the font list and in use.", v0 = name))
                    } else {
                        Ok(t!("{v0} is in the font list.", v0 = name))
                    }
                }
                None => Err(t!("The download wasn't a font Hoop can use.").to_string()),
            },
            Ok(path) => {
                if let Some(i) = self.fonts.iter().position(|f| matches!(&f.data, FontData::File(p, _) if *p == path)) {
                    self.fonts.remove(i);
                    if self.letter.font > i {
                        self.letter.font -= 1;
                    }
                }
                Ok(t!("{v0} was removed.", v0 = label))
            }
            Err(e) => Err(e),
        };
        self.fontwin.message = Some(message);
    }
}
