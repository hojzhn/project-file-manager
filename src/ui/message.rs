use std::path::PathBuf;

use crate::model::{ChildFileId, ProjectId};

#[derive(Debug, Clone)]
pub enum Message {
    Tick,

    ProjectSelected(ProjectId),

    NewProjectMenuOpened,
    NewProjectMenuClosed,
    PickImageFromDownloads,
    BrowseForImage,

    StartScratchProject,
    ScratchNameChanged(String),
    ScratchSubmitted,
    ScratchCancelled,

    OpenSetupDialog,
    SetupRootPicked,
    SetupRelevantDirAdded,
    SetupRelevantDirRemoved(usize),
    SetupParentExtensionsChanged(String),
    SetupChildExtensionsChanged(String),
    SetupSubmitted,

    SyncNowClicked,
    RescanProjectClicked,
    SyncRelevantDirectoriesClicked,
    MoveAllMatchedClicked,
    MoveChildFileClicked(ChildFileId),
    OpenFileClicked(PathBuf),
    RevealFileClicked(PathBuf),
    StatusDismissed,

    FilesDropped(PathBuf),
}
