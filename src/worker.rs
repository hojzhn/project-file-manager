use crossbeam_channel::{Receiver, Sender};

use crate::commands::{Command, UiEvent, WatchEvent};
use crate::db::{queries, Db};
use crate::error::{AppError, AppResult};
use crate::model::{ExtensionRules, ProjectId};
use crate::organize;
use crate::paths;
use crate::scanner;
use crate::watcher::Watchers;

pub fn spawn(
    command_rx: Receiver<Command>,
    watch_tx: Sender<WatchEvent>,
    watch_rx: Receiver<WatchEvent>,
    ui_event_tx: Sender<UiEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut db = match Db::open_default() {
            Ok(db) => db,
            Err(e) => {
                emit(&ui_event_tx, UiEvent::Error(format!("failed to open database: {e}")));
                return;
            }
        };
        let mut watchers = Watchers::new();

        send_settings(&db, &ui_event_tx);

        if let Ok(settings) = queries::get_settings(&db.conn) {
            if let Some(root) = &settings.root_directory {
                let _ = scanner::sync_root_directory(&mut db.conn, root, &settings.extension_rules);
                watchers.watch_root(root, watch_tx.clone());
            }
            if !settings.relevant_directories.is_empty() {
                let _ = scanner::scan_relevant_directories(
                    &mut db.conn,
                    &settings.relevant_directories,
                    &settings.extension_rules,
                );
                watchers.watch_relevant_directories(&settings.relevant_directories, watch_tx.clone());
            }
        }
        send_projects(&db, &ui_event_tx);

        loop {
            crossbeam_channel::select! {
                recv(command_rx) -> msg => match msg {
                    Ok(cmd) => handle_command(&mut db, cmd, &ui_event_tx, &mut watchers, &watch_tx),
                    Err(_) => break,
                },
                recv(watch_rx) -> msg => match msg {
                    Ok(WatchEvent::RootChanged) => {
                        if let Err(e) = sync_root_and_refresh(&mut db, &ui_event_tx) {
                            emit(&ui_event_tx, UiEvent::Error(e.to_string()));
                        }
                    }
                    Ok(WatchEvent::RelevantChanged) => {
                        if let Err(e) = sync_relevant_and_refresh(&mut db, &ui_event_tx) {
                            emit(&ui_event_tx, UiEvent::Error(e.to_string()));
                        }
                    }
                    Err(_) => {}
                },
            }
        }
    })
}

fn emit(tx: &Sender<UiEvent>, event: UiEvent) {
    let _ = tx.send(event);
}

fn send_settings(db: &Db, tx: &Sender<UiEvent>) {
    match queries::get_settings(&db.conn) {
        Ok(settings) => emit(tx, UiEvent::Settings(settings)),
        Err(e) => emit(tx, UiEvent::Error(e.to_string())),
    }
}

fn send_projects(db: &Db, tx: &Sender<UiEvent>) {
    match queries::list_projects(&db.conn) {
        Ok(projects) => emit(tx, UiEvent::ProjectsList(projects)),
        Err(e) => emit(tx, UiEvent::Error(e.to_string())),
    }
}

fn send_files_for_project(db: &Db, project_id: ProjectId, tx: &Sender<UiEvent>) -> AppResult<()> {
    let files = queries::files_for_project(&db.conn, project_id)?;
    emit(tx, UiEvent::FilesForProject { project_id, files });
    Ok(())
}

fn sync_root_and_refresh(db: &mut Db, tx: &Sender<UiEvent>) -> AppResult<()> {
    let settings = queries::get_settings(&db.conn)?;
    let root = settings.root_directory.ok_or(AppError::DirectoriesNotConfigured)?;
    scanner::sync_root_directory(&mut db.conn, &root, &settings.extension_rules)?;
    send_projects(db, tx);
    for project in queries::list_projects(&db.conn)? {
        send_files_for_project(db, project.id, tx)?;
    }
    Ok(())
}

fn sync_relevant_and_refresh(db: &mut Db, tx: &Sender<UiEvent>) -> AppResult<()> {
    let settings = queries::get_settings(&db.conn)?;
    if settings.relevant_directories.is_empty() {
        return Err(AppError::DirectoriesNotConfigured);
    }
    scanner::scan_relevant_directories(&mut db.conn, &settings.relevant_directories, &settings.extension_rules)?;
    for project in queries::list_projects(&db.conn)? {
        send_files_for_project(db, project.id, tx)?;
    }
    Ok(())
}

fn handle_command(db: &mut Db, cmd: Command, tx: &Sender<UiEvent>, watchers: &mut Watchers, watch_tx: &Sender<WatchEvent>) {
    if let Err(e) = run_command(db, cmd, tx, watchers, watch_tx) {
        emit(tx, UiEvent::Error(e.to_string()));
    }
}

fn run_command(
    db: &mut Db,
    cmd: Command,
    tx: &Sender<UiEvent>,
    watchers: &mut Watchers,
    watch_tx: &Sender<WatchEvent>,
) -> AppResult<()> {
    match cmd {
        Command::GetSettings => send_settings(db, tx),

        Command::SaveSettings { root_directory, relevant_directories, parent_extensions, child_extensions } => {
            let root_directory = paths::canonicalize(&root_directory)?;
            std::fs::create_dir_all(&root_directory)?;
            let relevant_directories = relevant_directories
                .iter()
                .map(|dir| paths::canonicalize(dir))
                .collect::<AppResult<Vec<_>>>()?;
            let extension_rules = ExtensionRules { parent_extensions, child_extensions };

            queries::set_root_directory(&db.conn, &root_directory)?;
            queries::replace_relevant_directories(&db.conn, &relevant_directories)?;
            queries::set_extension_rules(&db.conn, &extension_rules)?;

            let settings = queries::get_settings(&db.conn)?;
            emit(tx, UiEvent::Settings(settings.clone()));

            scanner::scan_relevant_directories(&mut db.conn, &settings.relevant_directories, &settings.extension_rules)?;
            scanner::sync_root_directory(&mut db.conn, &root_directory, &settings.extension_rules)?;
            watchers.watch_root(&root_directory, watch_tx.clone());
            watchers.watch_relevant_directories(&settings.relevant_directories, watch_tx.clone());

            send_projects(db, tx);
        }

        Command::ListProjects => send_projects(db, tx),

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

            scanner::scan_project_home(&mut db.conn, &project, &settings.extension_rules)?;
            queries::rematch_child_files(&db.conn)?;

            emit(tx, UiEvent::ProjectCreated(project.clone()));
            send_projects(db, tx);
            send_files_for_project(db, project.id, tx)?;
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

            scanner::scan_project_home(&mut db.conn, &project, &settings.extension_rules)?;

            emit(tx, UiEvent::ProjectCreated(project.clone()));
            send_projects(db, tx);
            send_files_for_project(db, project.id, tx)?;
        }

        Command::ListFilesForProject { project_id } => {
            send_files_for_project(db, project_id, tx)?;
        }

        Command::RescanProject { project_id } => {
            let project = queries::get_project(&db.conn, project_id)?.ok_or(AppError::ProjectNotFound(project_id))?;
            let settings = queries::get_settings(&db.conn)?;
            scanner::scan_project_home(&mut db.conn, &project, &settings.extension_rules)?;
            queries::rematch_child_files(&db.conn)?;
            send_files_for_project(db, project_id, tx)?;
        }

        Command::SyncRootDirectory => sync_root_and_refresh(db, tx)?,

        Command::SyncRelevantDirectories => sync_relevant_and_refresh(db, tx)?,

        Command::MoveChildFileIntoProject { child_file_id } => {
            let child_file = queries::get_child_file(&db.conn, child_file_id)?
                .ok_or(AppError::ChildFileNotFound(child_file_id))?;
            let project_id = child_file.matched_project_id.ok_or(AppError::ChildFileNotFound(child_file_id))?;
            let project = queries::get_project(&db.conn, project_id)?.ok_or(AppError::ProjectNotFound(project_id))?;
            let settings = queries::get_settings(&db.conn)?;

            organize::move_child_file_into_project(&mut db.conn, &child_file, &project)?;
            scanner::scan_project_home(&mut db.conn, &project, &settings.extension_rules)?;
            send_files_for_project(db, project_id, tx)?;
        }

        Command::MoveAllMatchedIntoProject { project_id } => {
            let project = queries::get_project(&db.conn, project_id)?.ok_or(AppError::ProjectNotFound(project_id))?;
            let settings = queries::get_settings(&db.conn)?;
            let matched = queries::child_files_matched_to_project(&db.conn, project_id)?;
            for child_file in &matched {
                organize::move_child_file_into_project(&mut db.conn, child_file, &project)?;
            }
            if !matched.is_empty() {
                scanner::scan_project_home(&mut db.conn, &project, &settings.extension_rules)?;
            }
            send_files_for_project(db, project_id, tx)?;
        }
    }
    Ok(())
}
