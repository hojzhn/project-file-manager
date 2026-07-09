use crossbeam_channel::{Receiver, Sender};
use eframe::egui::Context;

use crate::commands::{Command, UiEvent, WatchEvent};
use crate::db::{queries, Db};
use crate::error::{AppError, AppResult};
use crate::model::ProjectId;
use crate::organize;
use crate::paths;
use crate::scanner;
use crate::watcher::Watchers;

pub fn spawn(
    command_rx: Receiver<Command>,
    watch_tx: Sender<WatchEvent>,
    watch_rx: Receiver<WatchEvent>,
    ui_event_tx: Sender<UiEvent>,
    ctx: Context,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut db = match Db::open_default() {
            Ok(db) => db,
            Err(e) => {
                emit(&ui_event_tx, &ctx, UiEvent::Error(format!("failed to open database: {e}")));
                return;
            }
        };
        let mut watchers = Watchers::new();

        send_settings(&db, &ui_event_tx, &ctx);

        if let Ok(settings) = queries::get_settings(&db.conn) {
            if let Some(root) = &settings.root_directory {
                let _ = scanner::sync_root_directory(&mut db.conn, root);
                watchers.watch_root(root, watch_tx.clone());
            }
            if let Some(rip) = &settings.rip_directory {
                let _ = scanner::scan_rip_directory(&mut db.conn, rip);
                watchers.watch_rip(rip, watch_tx.clone());
            }
        }
        send_projects(&db, &ui_event_tx, &ctx);

        loop {
            crossbeam_channel::select! {
                recv(command_rx) -> msg => match msg {
                    Ok(cmd) => handle_command(&mut db, cmd, &ui_event_tx, &ctx, &mut watchers, &watch_tx),
                    Err(_) => break,
                },
                recv(watch_rx) -> msg => match msg {
                    Ok(WatchEvent::RootChanged) => {
                        if let Err(e) = sync_root_and_refresh(&mut db, &ui_event_tx, &ctx) {
                            emit(&ui_event_tx, &ctx, UiEvent::Error(e.to_string()));
                        }
                    }
                    Ok(WatchEvent::RipChanged) => {
                        if let Err(e) = sync_rip_and_refresh(&mut db, &ui_event_tx, &ctx) {
                            emit(&ui_event_tx, &ctx, UiEvent::Error(e.to_string()));
                        }
                    }
                    Err(_) => {}
                },
            }
        }
    })
}

fn emit(tx: &Sender<UiEvent>, ctx: &Context, event: UiEvent) {
    let _ = tx.send(event);
    ctx.request_repaint();
}

fn send_settings(db: &Db, tx: &Sender<UiEvent>, ctx: &Context) {
    match queries::get_settings(&db.conn) {
        Ok(settings) => emit(tx, ctx, UiEvent::Settings(settings)),
        Err(e) => emit(tx, ctx, UiEvent::Error(e.to_string())),
    }
}

fn send_projects(db: &Db, tx: &Sender<UiEvent>, ctx: &Context) {
    match queries::list_projects(&db.conn) {
        Ok(projects) => emit(tx, ctx, UiEvent::ProjectsList(projects)),
        Err(e) => emit(tx, ctx, UiEvent::Error(e.to_string())),
    }
}

fn send_files_for_project(db: &Db, project_id: ProjectId, tx: &Sender<UiEvent>, ctx: &Context) -> AppResult<()> {
    let files = queries::files_for_project(&db.conn, project_id)?;
    emit(tx, ctx, UiEvent::FilesForProject { project_id, files });
    Ok(())
}

fn sync_root_and_refresh(db: &mut Db, tx: &Sender<UiEvent>, ctx: &Context) -> AppResult<()> {
    let settings = queries::get_settings(&db.conn)?;
    let root = settings.root_directory.ok_or(AppError::DirectoriesNotConfigured)?;
    scanner::sync_root_directory(&mut db.conn, &root)?;
    send_projects(db, tx, ctx);
    for project in queries::list_projects(&db.conn)? {
        send_files_for_project(db, project.id, tx, ctx)?;
    }
    Ok(())
}

fn sync_rip_and_refresh(db: &mut Db, tx: &Sender<UiEvent>, ctx: &Context) -> AppResult<()> {
    let settings = queries::get_settings(&db.conn)?;
    let rip_dir = settings.rip_directory.ok_or(AppError::DirectoriesNotConfigured)?;
    scanner::scan_rip_directory(&mut db.conn, &rip_dir)?;
    for project in queries::list_projects(&db.conn)? {
        send_files_for_project(db, project.id, tx, ctx)?;
    }
    Ok(())
}

fn handle_command(
    db: &mut Db,
    cmd: Command,
    tx: &Sender<UiEvent>,
    ctx: &Context,
    watchers: &mut Watchers,
    watch_tx: &Sender<WatchEvent>,
) {
    if let Err(e) = run_command(db, cmd, tx, ctx, watchers, watch_tx) {
        emit(tx, ctx, UiEvent::Error(e.to_string()));
    }
}

fn run_command(
    db: &mut Db,
    cmd: Command,
    tx: &Sender<UiEvent>,
    ctx: &Context,
    watchers: &mut Watchers,
    watch_tx: &Sender<WatchEvent>,
) -> AppResult<()> {
    match cmd {
        Command::GetSettings => send_settings(db, tx, ctx),

        Command::SetDirectories { root_directory, rip_directory } => {
            let root_directory = paths::canonicalize(&root_directory)?;
            std::fs::create_dir_all(&root_directory)?;
            let rip_directory = paths::canonicalize(&rip_directory)?;
            let settings = queries::set_directories(&db.conn, &root_directory, &rip_directory)?;
            emit(tx, ctx, UiEvent::Settings(settings));

            scanner::scan_rip_directory(&mut db.conn, &rip_directory)?;
            scanner::sync_root_directory(&mut db.conn, &root_directory)?;
            watchers.watch_root(&root_directory, watch_tx.clone());
            watchers.watch_rip(&rip_directory, watch_tx.clone());

            send_projects(db, tx, ctx);
        }

        Command::ListProjects => send_projects(db, tx, ctx),

        Command::CreateProjectFromImage { image_path } => {
            let settings = queries::get_settings(&db.conn)?;
            let root = settings.root_directory.ok_or(AppError::DirectoriesNotConfigured)?;

            let stem = image_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "untitled".to_string());
            let orig_file_name = image_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| stem.clone());
            let seed_basename = scanner::base_name_of(&stem);

            let folder_path = paths::unique_project_folder(&db.conn, &root, &paths::with_date_prefix(&stem))?;
            std::fs::create_dir_all(&folder_path)?;

            let dest = organize::unique_destination(&folder_path, &orig_file_name);
            organize::move_file(&image_path, &dest)?;

            let name = folder_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(stem);
            let project = queries::create_project(
                &db.conn,
                &name,
                &folder_path,
                Some(&orig_file_name),
                Some(&seed_basename),
            )?;

            scanner::scan_project_home(&mut db.conn, &project)?;
            queries::rematch_rip_files(&db.conn)?;

            emit(tx, ctx, UiEvent::ProjectCreated(project.clone()));
            send_projects(db, tx, ctx);
            send_files_for_project(db, project.id, tx, ctx)?;
        }

        Command::CreateProjectFromScratch { name } => {
            let settings = queries::get_settings(&db.conn)?;
            let root = settings.root_directory.ok_or(AppError::DirectoriesNotConfigured)?;

            let folder_path = paths::unique_project_folder(&db.conn, &root, &paths::with_date_prefix(&name))?;
            std::fs::create_dir_all(&folder_path)?;

            let folder_name = folder_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(name);
            let project = queries::create_project(&db.conn, &folder_name, &folder_path, None, None)?;

            scanner::scan_project_home(&mut db.conn, &project)?;

            emit(tx, ctx, UiEvent::ProjectCreated(project.clone()));
            send_projects(db, tx, ctx);
            send_files_for_project(db, project.id, tx, ctx)?;
        }

        Command::ListFilesForProject { project_id } => {
            send_files_for_project(db, project_id, tx, ctx)?;
        }

        Command::RescanProject { project_id } => {
            let project = queries::get_project(&db.conn, project_id)?.ok_or(AppError::ProjectNotFound(project_id))?;
            scanner::scan_project_home(&mut db.conn, &project)?;
            send_files_for_project(db, project_id, tx, ctx)?;
        }

        Command::SyncRootDirectory => sync_root_and_refresh(db, tx, ctx)?,

        Command::RescanRipDirectory => sync_rip_and_refresh(db, tx, ctx)?,

        Command::MoveRipFileIntoProject { rip_file_id } => {
            let rip_file = queries::get_rip_file(&db.conn, rip_file_id)?
                .ok_or(AppError::RipFileNotFound(rip_file_id))?;
            let project_id = rip_file.matched_project_id.ok_or(AppError::RipFileNotFound(rip_file_id))?;
            let project = queries::get_project(&db.conn, project_id)?.ok_or(AppError::ProjectNotFound(project_id))?;

            organize::move_rip_file_into_project(&mut db.conn, &rip_file, &project)?;
            scanner::scan_project_home(&mut db.conn, &project)?;
            send_files_for_project(db, project_id, tx, ctx)?;
        }

        Command::MoveAllMatchedIntoProject { project_id } => {
            let project = queries::get_project(&db.conn, project_id)?.ok_or(AppError::ProjectNotFound(project_id))?;
            let matched = queries::rip_files_matched_to_project(&db.conn, project_id)?;
            for rip_file in &matched {
                organize::move_rip_file_into_project(&mut db.conn, rip_file, &project)?;
            }
            if !matched.is_empty() {
                scanner::scan_project_home(&mut db.conn, &project)?;
            }
            send_files_for_project(db, project_id, tx, ctx)?;
        }
    }
    Ok(())
}
