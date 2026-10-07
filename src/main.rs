mod app;
mod canvas;
mod formats;
mod hoops;
mod lettering;
mod theme;

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
        ..Default::default()
    };
    eframe::run_native("hoop", options, Box::new(|cc| Ok(Box::new(app::HoopApp::new(cc, file)))))
}
