use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crossbeam_channel::{Receiver, Sender};
use iced::widget::image::Handle as ImageHandle;

use crate::commands::{Command, UiEvent};
use crate::model::{Project, ProjectFileView, ProjectId, Settings};
use crate::ui::thumbnails::ThumbnailRequest;

pub struct SetupDialogState {
    pub root: Option<PathBuf>,
    pub relevant_dirs: Vec<PathBuf>,
    pub parent_extensions: String,
    pub child_extensions: String,
}

impl SetupDialogState {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            root: settings.root_directory.clone(),
            relevant_dirs: settings.relevant_directories.clone(),
            parent_extensions: settings.extension_rules.parent_extensions.join(", "),
            child_extensions: settings.extension_rules.child_extensions.join(", "),
        }
    }
}

pub struct ScratchDialogState {
    pub name: String,
}

pub struct State {
    pub(crate) command_tx: Sender<Command>,
    pub(crate) ui_event_rx: Receiver<UiEvent>,

    pub settings: Settings,
    pub projects: Vec<Project>,
    pub selected_project: Option<ProjectId>,
    pub files_by_project: HashMap<ProjectId, Vec<ProjectFileView>>,

    pub setup_dialog: Option<SetupDialogState>,
    pub new_project_menu_open: bool,
    pub scratch_dialog: Option<ScratchDialogState>,

    thumbnails: HashMap<PathBuf, ImageHandle>,
    pending_thumbnails: RefCell<HashSet<PathBuf>>,
    thumbnail_request_tx: Sender<ThumbnailRequest>,
    pub(crate) thumbnail_result_rx: Receiver<(PathBuf, Option<ImageHandle>)>,

    pub status: Option<String>,
}

impl State {
    pub fn new(
        command_tx: Sender<Command>,
        ui_event_rx: Receiver<UiEvent>,
        thumbnail_request_tx: Sender<ThumbnailRequest>,
        thumbnail_result_rx: Receiver<(PathBuf, Option<ImageHandle>)>,
    ) -> Self {
        Self {
            command_tx,
            ui_event_rx,
            settings: Settings::default(),
            projects: Vec::new(),
            selected_project: None,
            files_by_project: HashMap::new(),
            setup_dialog: None,
            new_project_menu_open: false,
            scratch_dialog: None,
            thumbnails: HashMap::new(),
            pending_thumbnails: RefCell::new(HashSet::new()),
            thumbnail_request_tx,
            thumbnail_result_rx,
            status: None,
        }
    }

    pub(crate) fn send(&self, cmd: Command) {
        let _ = self.command_tx.send(cmd);
    }

    /// Returns the cached thumbnail for `file` if it's ready. If not, kicks off (or confirms
    /// in-flight) a background load; callable from `view()` since it only needs `&self`.
    pub fn thumbnail(&self, file: &ProjectFileView) -> Option<ImageHandle> {
        let path = &file.abs_path;
        if let Some(tex) = self.thumbnails.get(path) {
            return Some(tex.clone());
        }
        if self.pending_thumbnails.borrow_mut().insert(path.clone()) {
            let _ = self.thumbnail_request_tx.send(ThumbnailRequest {
                path: path.clone(),
                modified_unix: file.modified_at.timestamp(),
                size_bytes: file.size_bytes,
            });
        }
        None
    }

    pub(crate) fn drain_thumbnail_results(&mut self) {
        while let Ok((path, tex)) = self.thumbnail_result_rx.try_recv() {
            self.pending_thumbnails.borrow_mut().remove(&path);
            if let Some(tex) = tex {
                self.thumbnails.insert(path, tex);
            }
        }
    }
}
