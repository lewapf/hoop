//! Help → User manual (F1): docs/MANUAL.md, built in and shown with a
//! small Markdown reader (headings, lists, tables, `code`, **bold**).

use super::*;

const MANUAL: &str = include_str!("../../docs/MANUAL.md");

#[derive(Default)]
pub(super) struct Manual {
    pub open: bool,
    /// A section picked in the contents, scrolled to on the next frame.
    jump: Option<usize>,
}

/// Text with `code` and **bold** spans as one wrapped label.
fn rich(ui: &mut egui::Ui, line: &str, size: f32) {
    let mut job = egui::text::LayoutJob::default();
    let color = ui.visuals().text_color();
    let strong = ui.visuals().strong_text_color();
    let mut rest = line;
    let body = egui::FontId::proportional(size);
    let mono = egui::FontId::monospace(size - 1.0);
    let fmt = |font: egui::FontId, color: Color32| egui::TextFormat { font_id: font, color, ..Default::default() };
    while !rest.is_empty() {
        let code = rest.find('`');
        let bold = rest.find("**");
        match (code, bold) {
            (Some(c), b) if b.is_none_or(|b| c < b) => {
                job.append(&rest[..c], 0.0, fmt(body.clone(), color));
                let after = &rest[c + 1..];
                let end = after.find('`').unwrap_or(after.len());
                job.append(&after[..end], 0.0, fmt(mono.clone(), strong));
                rest = after.get(end + 1..).unwrap_or("");
            }
            (_, Some(b)) => {
                job.append(&rest[..b], 0.0, fmt(body.clone(), color));
                let after = &rest[b + 2..];
                let end = after.find("**").unwrap_or(after.len());
                job.append(&after[..end], 0.0, fmt(body.clone(), strong));
                rest = after.get(end + 2..).unwrap_or("");
            }
            _ => {
                job.append(rest, 0.0, fmt(body.clone(), color));
                rest = "";
            }
        }
    }
    job.wrap.max_width = ui.available_width();
    ui.label(job);
}

enum Block {
    Title(String),
    Section(String),
    Para(String),
    /// Indent level, bullet or number, text.
    Item(usize, String, String),
    Table(Vec<Vec<String>>),
}

/// The manual as blocks: lines of a paragraph or list item joined.
fn blocks() -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut open = false; // the last block can take more lines
    for line in MANUAL.lines() {
        let indent = line.len() - line.trim_start().len();
        let l = line.trim();
        if l.is_empty() {
            open = false;
            continue;
        }
        if let Some(h) = l.strip_prefix("# ") {
            out.push(Block::Title(h.into()));
            open = false;
        } else if let Some(h) = l.strip_prefix("## ") {
            out.push(Block::Section(h.into()));
            open = false;
        } else if l.starts_with('|') {
            if l.contains("---") {
                continue;
            }
            let row: Vec<String> = l.trim_matches('|').split('|').map(|c| c.trim().to_string()).collect();
            match out.last_mut() {
                Some(Block::Table(rows)) => rows.push(row),
                _ => out.push(Block::Table(vec![row])),
            }
            open = false;
        } else if let Some(t) = l.strip_prefix("- ") {
            out.push(Block::Item(indent / 2, "•".into(), t.into()));
            open = true;
        } else if let Some((n, t)) = l.split_once(". ").filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) {
            out.push(Block::Item(indent / 3, format!("{n}."), t.into()));
            open = true;
        } else if open {
            match out.last_mut() {
                Some(Block::Para(t)) | Some(Block::Item(_, _, t)) => {
                    t.push(' ');
                    t.push_str(l);
                }
                _ => out.push(Block::Para(l.into())),
            }
        } else {
            out.push(Block::Para(l.into()));
            open = true;
        }
    }
    out
}

impl HoopApp {
    pub(super) fn manual_window(&mut self, ctx: &egui::Context) {
        if !self.manual.open {
            return;
        }
        let mut open = true;
        let accent = self.accent();
        let sections: Vec<&str> = MANUAL.lines().filter_map(|l| l.strip_prefix("## ")).collect();
        let mut jump = self.manual.jump.take();
        egui::Window::new(t!("User manual")).open(&mut open).default_size(vec2(780.0, 640.0)).max_width(ctx.content_rect().width() - 80.0).resizable(true).collapsible(false).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                // Contents.
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    for (k, s) in sections.iter().enumerate() {
                        if ui.add(egui::Button::new(RichText::new(*s).size(13.0)).frame(false)).clicked() {
                            jump = Some(k);
                        }
                    }
                });
                ui.separator();
                let width = ui.available_width() - 12.0;
                egui::ScrollArea::vertical().id_salt("manual").auto_shrink([false, false]).show(ui, |ui| ui.vertical(|ui| {
                    ui.set_width(width);
                    let mut section = 0;
                    for b in blocks() {
                        match b {
                            Block::Title(h) => {
                                ui.label(RichText::new(h).size(24.0).strong());
                            }
                            Block::Section(h) => {
                                ui.add_space(12.0);
                                let r = ui.label(RichText::new(h).size(18.0).strong().color(accent));
                                if jump == Some(section) {
                                    r.scroll_to_me(Some(egui::Align::TOP));
                                }
                                section += 1;
                            }
                            Block::Para(t) => {
                                rich(ui, &t, 14.0);
                                ui.add_space(4.0);
                            }
                            Block::Item(depth, mark, t) => {
                                ui.horizontal_top(|ui| {
                                    ui.add_space(depth as f32 * 18.0);
                                    ui.label(mark);
                                    rich(ui, &t, 14.0);
                                });
                            }
                            Block::Table(rows) => {
                                egui::Grid::new(("manual-table", section)).striped(true).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
                                    for (r, row) in rows.iter().enumerate() {
                                        for cell in row {
                                            if r == 0 {
                                                ui.label(RichText::new(cell).strong());
                                            } else {
                                                rich(ui, cell, 13.5);
                                            }
                                        }
                                        ui.end_row();
                                    }
                                });
                            }
                        }
                    }
                }));
            });
        });
        if !open {
            self.manual.open = false;
        }
    }
}
