pub mod migrations;
pub mod queries;

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn open_default() -> AppResult<Self> {
        let path = default_db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::open(&path)
    }

    pub fn open(path: &Path) -> AppResult<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        migrations::runner().to_latest(&mut conn)?;
        Ok(Self { conn })
    }
}

fn project_dirs() -> AppResult<directories::ProjectDirs> {
    directories::ProjectDirs::from("dev", "artmatr-engineering", "matr-project-file-manager")
        .ok_or(AppError::NoProjectDirs)
}

pub fn default_db_path() -> AppResult<PathBuf> {
    Ok(project_dirs()?.data_local_dir().join("index.sqlite3"))
}

pub fn thumbnail_cache_dir() -> AppResult<PathBuf> {
    Ok(project_dirs()?.data_local_dir().join("thumbnails"))
}
