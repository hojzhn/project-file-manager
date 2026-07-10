use matr_project_file_manager::{commands, db, ui, worker};

fn main() -> iced::Result {
    tracing_subscriber::fmt::init();

    let (command_tx, command_rx) = crossbeam_channel::unbounded();
    let (watch_tx, watch_rx) = crossbeam_channel::unbounded::<commands::WatchEvent>();
    let (ui_event_tx, ui_event_rx) = crossbeam_channel::unbounded();

    worker::spawn(command_rx, watch_tx, watch_rx, ui_event_tx);

    let cache_dir = db::thumbnail_cache_dir().ok();
    let (thumbnail_request_tx, thumbnail_result_rx) = ui::thumbnails::spawn_loader(cache_dir);

    iced::application(
        move || {
            ui::state::State::new(
                command_tx.clone(),
                ui_event_rx.clone(),
                thumbnail_request_tx.clone(),
                thumbnail_result_rx.clone(),
            )
        },
        ui::update::update,
        ui::view::view,
    )
    .title("Matr Project File Manager")
    .subscription(ui::subscription::subscription)
    .run()
}
