//! Getting designs out: pinning parts together, the stitches on the hoop,
//! Save as JEF, export, worksheet, the picture of the preview, and the
//! save dialog.

use super::*;

impl HoopApp {
    /// Fixes the current design on the hoop as part of a combined design.
    pub(super) fn pin(&mut self) {
        let Some(d) = &self.design else { return };
        let t = Transform::new(d.bounds, &self.setup.at);
        let points: Vec<(f32, f32)> = d.pattern.points.iter().map(|&p| t.apply(p)).collect();
        let mut pattern = d.pattern.clone();
        pattern.points = points;
        pattern.threads = d.blocks.iter().map(|b| b.thread.clone()).collect();
        let blocks = pattern.blocks();
        let bounds = pattern.bounds();
        let height = bounds.3 - bounds.1;
        self.pinned.push(Pinned { name: d.name.clone(), pattern, blocks, bounds });
        // Start the next part just below, so the two don't sit on top of each other.
        self.change(|s| s.at.dy += height + 50.0);
        self.toast(t!("Pinned to the hoop. Make or open the next part.").into());
    }

    /// World-space bounds of everything on the hoop, pinned parts included.
    pub(super) fn hoop_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut b: Option<(f32, f32, f32, f32)> = None;
        let mut add = |r: (f32, f32, f32, f32)| {
            b = Some(match b {
                None => r,
                Some(o) => (o.0.min(r.0), o.1.min(r.1), o.2.max(r.2), o.3.max(r.3)),
            });
        };
        for p in &self.pinned {
            add(p.bounds);
        }
        if let Some(d) = &self.design {
            let (w, h) = self.setup.at.size(d.size().0, d.size().1);
            let at = self.setup.at;
            add((at.dx - w / 2.0, at.dy - h / 2.0, at.dx + w / 2.0, at.dy + h / 2.0));
        }
        b
    }

    pub(super) fn save_jef(&mut self) {
        self.export(Out::Jef);
    }

    /// Saves what's on the hoop in `out`'s format.
    pub(super) fn export(&mut self, out: Out) {
        if !self.checked(checks::Output::Save(out)) {
            return;
        }
        let Some((points, cmds, threads)) = self.hoop_stitches() else { return };
        let stem = if self.pinned.is_empty() { self.design_stem() } else { format!("{}-combined", self.design_stem()) };
        let bytes = export::write(out, &stem, &points, &cmds, &threads);
        let title = if out == Out::Jef { t!("Save as JEF") } else { t!("Export") };
        self.ask_save(title, format!("{stem}.{}", out.ext()), out.filter(), out.ext(), bytes);
    }

    /// Saves a PDF worksheet: facts, thread sequence and an actual-size template.
    pub(super) fn save_worksheet(&mut self) {
        let Some((points, cmds, threads)) = self.hoop_stitches() else { return };
        let hoop = HOOPS[self.setup.hoop];
        let stem = self.design_stem();
        let name = self.design.as_ref().map_or(stem.clone(), |d| d.name.clone());
        let threads: Vec<formats::Thread> = threads.iter().map(|t| self.chart_thread(t).unwrap_or_else(|| t.clone())).collect();
        let chart = self.chart().name.clone();
        let bytes = crate::worksheet::pdf(&crate::worksheet::Sheet {
            chart: &chart,
            name: &name,
            hoop: &t!("{name} {w} × {h} mm", name = hoop.name, w = hoop.w, h = hoop.h),
            points: &points,
            cmds: &cmds,
            threads: &threads,
            spm: SPEEDS[self.speed],
            date: &formats::today(),
        });
        self.ask_save(t!("Save worksheet"), format!("{stem}-worksheet.pdf"), "PDF", "pdf", bytes);
    }

    /// The JEF for what's on the hoop: the design, or every pinned part with it.
    pub(super) fn jef_bytes(&self) -> Option<Vec<u8>> {
        let (points, cmds, threads) = self.hoop_stitches()?;
        Some(formats::write_jef(&points, &cmds, &export::jef_numbers(&threads)))
    }

    /// What's on the hoop as points centered on the hoop, commands and one
    /// thread per color block.
    pub(super) fn hoop_stitches(&self) -> Option<(Vec<(f32, f32)>, Vec<Cmd>, Vec<formats::Thread>)> {
        if !self.pinned.is_empty() {
            return self.combined_stitches();
        }
        let d = self.design.as_ref()?;
        // Rotation and mirroring are baked in; the offset isn't, because the
        // machine positions designs from the hoop center itself.
        let at = Placement { dx: 0.0, dy: 0.0, ..self.setup.at };
        let t = Transform::new(d.bounds, &at);
        let points: Vec<(f32, f32)> = d.pattern.points.iter().map(|&p| t.apply(p)).collect();
        Some((points, d.pattern.cmds.clone(), d.blocks.iter().map(|b| b.thread.clone()).collect()))
    }

    /// The pinned parts and the current design as one design, keeping their
    /// places relative to each other and centering the whole on the hoop.
    pub(super) fn combined_stitches(&self) -> Option<(Vec<(f32, f32)>, Vec<Cmd>, Vec<formats::Thread>)> {
        let mut points: Vec<(f32, f32)> = Vec::new();
        let mut cmds: Vec<Cmd> = Vec::new();
        let mut threads: Vec<formats::Thread> = Vec::new();
        let mut parts: Vec<(Vec<(f32, f32)>, Vec<Cmd>, Vec<formats::Thread>)> =
            self.pinned.iter().map(|p| (p.pattern.points.clone(), p.pattern.cmds.clone(), p.blocks.iter().map(|b| b.thread.clone()).collect())).collect();
        if let Some(d) = &self.design {
            let t = Transform::new(d.bounds, &self.setup.at);
            parts.push((d.pattern.points.iter().map(|&p| t.apply(p)).collect(), d.pattern.cmds.clone(), d.blocks.iter().map(|b| b.thread.clone()).collect()));
        }
        for (pts, cs, ts) in parts {
            if ts.is_empty() || pts.is_empty() {
                continue;
            }
            // A thread change between parts unless the next part starts in the
            // thread the last one ended with.
            if let Some(last) = threads.last() {
                if *last != ts[0] {
                    let at = *points.last().unwrap();
                    points.push(at);
                    cmds.push(Cmd::ColorChange);
                    threads.push(ts[0].clone());
                }
                threads.extend(ts.into_iter().skip(1));
            } else {
                threads.extend(ts);
            }
            // Each part starts by moving to its first point.
            points.push(pts[0]);
            cmds.push(Cmd::Jump);
            points.extend(pts.into_iter().skip(1));
            cmds.extend(cs.into_iter().skip(1));
        }
        let b = self.hoop_bounds()?;
        let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
        let centered: Vec<(f32, f32)> = points.iter().map(|&(x, y)| (x - cx, y - cy)).collect();
        Some((centered, cmds, threads))
    }

    /// A file name for the shown design, without extension.
    pub(super) fn design_stem(&self) -> String {
        let Some(d) = &self.design else { return "design".into() };
        let stem: String = match self.mode {
            Mode::Lettering => d.name.chars().filter(|c| c.is_alphanumeric() || *c == ' ').take(24).collect::<String>().trim().replace(' ', "_"),
            Mode::Artwork => d.name.rsplit_once('.').map_or(d.name.as_str(), |(s, _)| s).to_string(),
            Mode::Viewer => {
                let stem = d.name.rsplit_once('.').map_or(d.name.as_str(), |(s, _)| s).to_string();
                if d.pattern.format == formats::Format::Jef { format!("{stem}-hoop") } else { stem }
            }
        };
        if stem.is_empty() { "lettering".into() } else { stem }
    }

    pub(super) fn ask_save(&mut self, title: &'static str, name: String, filter: &'static str, ext: &'static str, bytes: Vec<u8>) {
        if self.saver.is_some() {
            return;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let dir = [home.join("Documents/Embroidery Designs"), home.join("Documents"), home]
            .into_iter()
            .find(|p| p.is_dir())
            .unwrap_or_default();
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new().set_title(title).set_directory(dir).set_file_name(name).add_filter(filter, &[ext]).save_file();
            let _ = tx.send(file);
            ctx.request_repaint();
        });
        self.saver = Some((rx, bytes, ext));
    }

    pub(super) fn save_picture(&mut self) {
        if self.design.is_some() && self.capture == Capture::Idle && self.saver.is_none() {
            self.capture = Capture::Requested;
            self.ctx.request_repaint();
        }
    }

    /// Crops the screenshot to the canvas and asks where to save it as PNG.
    pub(super) fn finish_picture(&mut self, image: &egui::ColorImage, rect: egui::Rect, ppp: f32) {
        let [iw, ih] = image.size;
        let x0 = ((rect.min.x * ppp).round() as usize).min(iw);
        let y0 = ((rect.min.y * ppp).round() as usize).min(ih);
        let x1 = ((rect.max.x * ppp).round() as usize).min(iw);
        let y1 = ((rect.max.y * ppp).round() as usize).min(ih);
        let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        if w == 0 || h == 0 {
            return;
        }
        let mut rgba = Vec::with_capacity(w * h * 4);
        for y in y0..y1 {
            for p in &image.pixels[y * iw + x0..y * iw + x1] {
                rgba.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
            }
        }
        let mut png_bytes = Vec::new();
        let mut enc = png::Encoder::new(&mut png_bytes, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let written = enc.write_header().and_then(|mut wr| wr.write_image_data(&rgba));
        match written {
            Ok(()) => {
                let name = format!("{}.png", self.design_stem());
                self.ask_save(t!("Save picture"), name, t!("PNG picture"), "png", png_bytes);
            }
            Err(e) => self.error = Some(t!("Couldn't make the picture: {e}.", e = e)),
        }
    }

    pub(super) fn write_file(&mut self, mut path: PathBuf, bytes: &[u8], ext: &str) {
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            path.set_extension(ext);
        }
        // Write beside the target and rename, so a failed save never leaves half a file.
        let tmp = path.with_file_name(format!(".{}.tmp", path.file_name().unwrap_or_default().to_string_lossy()));
        let result = std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &path));
        match result {
            Ok(()) => {
                self.toast(t!("Saved {path}", path = path.display()));
                self.error = None;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.error = Some(t!("Couldn't save {v0}: {e}.", v0 = path.display(), e = e));
            }
        }
    }
}
