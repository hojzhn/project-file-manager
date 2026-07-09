use matr_project_file_manager::{app, commands, worker};

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();

    let (command_tx, command_rx) = crossbeam_channel::unbounded();
    let (watch_tx, watch_rx) = crossbeam_channel::unbounded::<commands::WatchEvent>();
    let (ui_event_tx, ui_event_rx) = crossbeam_channel::unbounded();

    eframe::run_native(
        "Matr Project File Manager",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            worker::spawn(command_rx, watch_tx, watch_rx, ui_event_tx, cc.egui_ctx.clone());
            Ok(Box::new(app::App::new(command_tx, ui_event_rx)))
        }),
    )
}
