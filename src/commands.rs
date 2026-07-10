use std::path::PathBuf;

use crate::model::{Project, ProjectFileView, ProjectId, RipFileId, Settings};

#[derive(Debug, Clone)]
pub enum Command {
    GetSettings,
    SaveSettings {
        root_directory: PathBuf,
        relevant_directories: Vec<PathBuf>,
        parent_extensions: Vec<String>,
        child_extensions: Vec<String>,
    },
    ListProjects,
    CreateProjectFromImage {
        image_path: PathBuf,
    },
    CreateProjectFromScratch {
        name: String,
    },
    ListFilesForProject {
        project_id: ProjectId,
    },
    RescanProject {
        project_id: ProjectId,
    },
    SyncRootDirectory,
    SyncRelevantDirectories,
    MoveRipFileIntoProject {
        rip_file_id: RipFileId,
    },
    MoveAllMatchedIntoProject {
        project_id: ProjectId,
    },
}

#[derive(Debug, Clone)]
pub enum UiEvent {
    Settings(Settings),
    ProjectsList(Vec<Project>),
    ProjectCreated(Project),
    FilesForProject {
        project_id: ProjectId,
        files: Vec<ProjectFileView>,
    },
    Error(String),
}

#[derive(Debug, Clone)]
pub enum WatchEvent {
    RootChanged,
    RelevantChanged,
}
