#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no project directories available for this platform")]
    NoProjectDirs,

    #[error("project not found: {0:?}")]
    ProjectNotFound(crate::model::ProjectId),

    #[error("child file not found: {0:?}")]
    ChildFileNotFound(crate::model::ChildFileId),

    #[error("root and relevant directories must be configured first")]
    DirectoriesNotConfigured,
}

pub type AppResult<T> = Result<T, AppError>;
