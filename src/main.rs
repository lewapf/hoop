mod app;
mod artwork;
mod canvas;
mod check;
mod export;
mod formats;
mod hoops;
mod lettering;
mod prefs;
mod route;
mod theme;
mod trace;
mod testsew;
mod usb;

fn main() -> eframe::Result {
    let file = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("hoop")
            .with_title("Hoop")
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([720.0, 480.0]),
        // Stitches are drawn as plain triangles, so multisampling smooths their edges.
        multisampling: 4,
        // With vsync, presenting a frame blocks while the window is hidden on
        // Wayland, and the compositor then reports Hoop as not responding.
        glow_options: eframe::egui_glow::GlowConfiguration { vsync: false, ..Default::default() },
        ..Default::default()
    };
    eframe::run_native("hoop", options, Box::new(|cc| Ok(Box::new(app::HoopApp::new(cc, file)))))
}
