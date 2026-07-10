use std::path::Path;

use crate::commands::{Command, UiEvent};
use crate::model::ExtensionRules;
use crate::ui::message::Message;
use crate::ui::state::{ScratchDialogState, SetupDialogState, State};

pub fn update(state: &mut State, message: Message) {
    match message {
        Message::Tick => {
            drain_events(state);
            state.drain_thumbnail_results();
        }

        Message::ProjectSelected(project_id) => {
            state.selected_project = Some(project_id);
            state.send(Command::ListFilesForProject { project_id });
        }

        Message::NewProjectMenuOpened => state.new_project_menu_open = true,
        Message::NewProjectMenuClosed => state.new_project_menu_open = false,

        Message::PickImageFromDownloads => {
            let parent_exts: Vec<&str> = state.settings.extension_rules.parent_extensions.iter().map(String::as_str).collect();
            let mut dialog = rfd::FileDialog::new().add_filter("Image", &parent_exts);
            if let Some(dir) = downloads_dir() {
                dialog = dialog.set_directory(dir);
            }
            if let Some(path) = dialog.pick_file() {
                state.send(Command::CreateProjectFromImage { image_path: path });
            }
            state.new_project_menu_open = false;
        }

        Message::BrowseForImage => {
            let parent_exts: Vec<&str> = state.settings.extension_rules.parent_extensions.iter().map(String::as_str).collect();
            if let Some(path) = rfd::FileDialog::new().add_filter("Image", &parent_exts).pick_file() {
                state.send(Command::CreateProjectFromImage { image_path: path });
            }
            state.new_project_menu_open = false;
        }

        Message::StartScratchProject => {
            state.scratch_dialog = Some(ScratchDialogState { name: String::new() });
            state.new_project_menu_open = false;
        }
        Message::ScratchNameChanged(name) => {
            if let Some(dialog) = &mut state.scratch_dialog {
                dialog.name = name;
            }
        }
        Message::ScratchSubmitted => {
            if let Some(dialog) = state.scratch_dialog.take() {
                let name = dialog.name.trim().to_string();
                if !name.is_empty() {
                    state.send(Command::CreateProjectFromScratch { name });
                }
            }
        }
        Message::ScratchCancelled => state.scratch_dialog = None,

        Message::OpenSetupDialog => {
            state.setup_dialog = Some(SetupDialogState::from_settings(&state.settings));
        }
        Message::SetupRootPicked => {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                if let Some(dialog) = &mut state.setup_dialog {
                    dialog.root = Some(path);
                }
            }
        }
        Message::SetupRelevantDirAdded => {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                if let Some(dialog) = &mut state.setup_dialog {
                    if !dialog.relevant_dirs.contains(&path) {
                        dialog.relevant_dirs.push(path);
                    }
                }
            }
        }
        Message::SetupRelevantDirRemoved(index) => {
            if let Some(dialog) = &mut state.setup_dialog {
                if index < dialog.relevant_dirs.len() {
                    dialog.relevant_dirs.remove(index);
                }
            }
        }
        Message::SetupParentExtensionsChanged(value) => {
            if let Some(dialog) = &mut state.setup_dialog {
                dialog.parent_extensions = value;
            }
        }
        Message::SetupChildExtensionsChanged(value) => {
            if let Some(dialog) = &mut state.setup_dialog {
                dialog.child_extensions = value;
            }
        }
        Message::SetupSubmitted => {
            if let Some(dialog) = &state.setup_dialog {
                if let Some(root) = dialog.root.clone() {
                    if !dialog.relevant_dirs.is_empty() {
                        state.send(Command::SaveSettings {
                            root_directory: root,
                            relevant_directories: dialog.relevant_dirs.clone(),
                            parent_extensions: parse_ext_list(&dialog.parent_extensions),
                            child_extensions: parse_ext_list(&dialog.child_extensions),
                        });
                    }
                }
            }
        }

        Message::SyncNowClicked => {
            state.send(Command::SyncRootDirectory);
            state.send(Command::SyncRelevantDirectories);
        }
        Message::RescanProjectClicked => {
            if let Some(project_id) = state.selected_project {
                state.send(Command::RescanProject { project_id });
            }
        }
        Message::SyncRelevantDirectoriesClicked => state.send(Command::SyncRelevantDirectories),
        Message::MoveAllMatchedClicked => {
            if let Some(project_id) = state.selected_project {
                state.send(Command::MoveAllMatchedIntoProject { project_id });
            }
        }
        Message::MoveChildFileClicked(child_file_id) => {
            state.send(Command::MoveChildFileIntoProject { child_file_id });
        }
        Message::OpenFileClicked(path) => {
            if let Err(e) = open::that(&path) {
                state.status = Some(format!("Couldn't open {}: {e}", path.display()));
            }
        }
        Message::RevealFileClicked(path) => {
            if let Err(e) = reveal_in_explorer(&path) {
                state.status = Some(format!("Couldn't reveal {}: {e}", path.display()));
            }
        }
        Message::StatusDismissed => state.status = None,

        Message::FilesDropped(path) => {
            if is_source_path(&state.settings.extension_rules, &path) {
                state.send(Command::CreateProjectFromImage { image_path: path });
            } else {
                state.status = Some(format!("Dropped file isn't a recognized image: {}", path.display()));
            }
        }
    }
}

fn drain_events(state: &mut State) {
    while let Ok(event) = state.ui_event_rx.try_recv() {
        match event {
            UiEvent::Settings(settings) => {
                if settings.is_configured() {
                    state.setup_dialog = None;
                } else if state.setup_dialog.is_none() {
                    state.setup_dialog = Some(SetupDialogState::from_settings(&settings));
                }
                state.settings = settings;
            }
            UiEvent::ProjectsList(projects) => state.projects = projects,
            UiEvent::ProjectCreated(project) => {
                state.selected_project = Some(project.id);
                if !state.projects.iter().any(|p| p.id == project.id) {
                    state.projects.push(project);
                }
            }
            UiEvent::FilesForProject { project_id, files } => {
                state.files_by_project.insert(project_id, files);
            }
            UiEvent::Error(message) => state.status = Some(message),
        }
    }
}

fn parse_ext_list(s: &str) -> Vec<String> {
    s.split(',').map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase()).filter(|e| !e.is_empty()).collect()
}

fn is_source_path(rules: &ExtensionRules, path: &Path) -> bool {
    path.extension().map(|e| rules.is_parent(&e.to_string_lossy().to_ascii_lowercase())).unwrap_or(false)
}

fn downloads_dir() -> Option<std::path::PathBuf> {
    directories::UserDirs::new().and_then(|u| u.download_dir().map(|p| p.to_path_buf()))
}

fn reveal_in_explorer(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn().map(|_| ())
}
