//! The Send to USB window.

use super::*;

impl HoopApp {
    pub(super) fn open_usb(&mut self) {
        if self.design.is_none() && self.pinned.is_empty() {
            return;
        }
        if !self.checked(checks::Output::Usb) {
            return;
        }
        self.show_usb(&self.design_stem(), None);
    }

    /// Opens the Send to USB window for the design, or for `bytes` instead.
    pub(super) fn show_usb(&mut self, name: &str, bytes: Option<Vec<u8>>) {
        self.usb.open = true;
        self.usb.drives = usb::drives();
        self.usb.chosen = 0;
        self.usb.message = None;
        self.usb.name = usb::short_name(name);
        self.usb.bytes = bytes;
    }

    pub(super) fn usb_window(&mut self, ctx: &egui::Context) {
        if !self.usb.open {
            return;
        }
        if let Some(rx) = &self.usb.job {
            if let Ok(result) = rx.try_recv() {
                self.usb.job = None;
                if let Ok(msg) = &result {
                    self.toast(msg.clone());
                }
                self.usb.message = Some(result);
                self.usb.drives = usb::drives();
            }
        }
        let accent = self.accent();
        let muted = self.muted();
        let mut open = true;
        let mut send = false;
        let mut eject = None;
        egui::Window::new(t!("Send to USB")).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(
            ctx,
            |ui| {
                ui.set_width(360.0);
                let busy = self.usb.job.is_some();
                if self.usb.drives.is_empty() {
                    ui.label(t!("Plug in the USB stick for the machine."));
                    ui.label(RichText::new(t!("Hoop looks for removable FAT, exFAT and NTFS drives.")).size(12.0).color(muted));
                } else {
                    for (i, d) in self.usb.drives.iter().enumerate() {
                        let state = if d.mount.is_some() { "ready" } else { t!("will be opened") };
                        ui.radio_value(&mut self.usb.chosen, i, format!("{} · {state}", d.title()));
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(t!("Name"));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.usb.name).desired_width(110.0).char_limit(8));
                    if r.changed() {
                        self.usb.name = usb::short_name(&self.usb.name);
                    }
                    ui.label(RichText::new(".jef").monospace().color(muted));
                });
                ui.label(
                    RichText::new(t!("Saved into EMB/Embf, where the MC9900 looks for designs. Up to 8 letters shows best on the machine."))
                        .size(12.0)
                        .color(muted),
                );
                if let Some(msg) = &self.usb.message {
                    match msg {
                        Ok(m) => ui.label(RichText::new(m).color(OK)),
                        Err(e) => ui.label(RichText::new(e).color(BAD)),
                    };
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let can_send = !busy && !self.usb.drives.is_empty();
                    if ui.add_enabled(can_send, primary(if busy { t!("Working…") } else { t!("Send") }, accent)).clicked() {
                        send = true;
                    }
                    if ui.add_enabled(!busy, egui::Button::new(t!("Look again"))).clicked() {
                        self.usb.drives = usb::drives();
                        self.usb.chosen = 0;
                    }
                    if let Some(d) = &self.usb.sent_to {
                        if ui.add_enabled(!busy, egui::Button::new("Eject")).on_hover_text(t!("Close the drive so it can be pulled out safely")).clicked() {
                            eject = Some(d.clone());
                        }
                    }
                });
            },
        );
        if !open {
            self.usb.open = false;
        }
        if send {
            if let (Some(drive), Some(bytes)) = (self.usb.drives.get(self.usb.chosen).cloned(), self.usb.bytes.clone().or_else(|| self.jef_bytes())) {
                let file = format!("{}.jef", usb::short_name(&self.usb.name));
                let (tx, rx) = channel();
                let ctx = ctx.clone();
                self.usb.sent_to = Some(drive.clone());
                std::thread::spawn(move || {
                    let r = usb::send(&drive, &file, &bytes)
                        .map(|p| t!("Sent {file} to {v1}", file = file, v1 = p.parent().map(|d| d.display().to_string()).unwrap_or_default()));
                    let _ = tx.send(r);
                    ctx.request_repaint();
                });
                self.usb.job = Some(rx);
                self.usb.message = None;
            }
        }
        if let Some(drive) = eject {
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r = usb::eject(&drive).map(|_| t!("{v0} is ejected. You can pull it out.", v0 = drive.title()));
                let _ = tx.send(r);
                ctx.request_repaint();
            });
            self.usb.sent_to = None;
            self.usb.job = Some(rx);
        }
    }
}
