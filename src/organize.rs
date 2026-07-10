use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::db::queries;
use crate::error::AppResult;
use crate::model::{ChildFile, Project};

pub fn move_child_file_into_project(
    conn: &mut Connection,
    child_file: &ChildFile,
    project: &Project,
) -> AppResult<PathBuf> {
    std::fs::create_dir_all(&project.folder_path)?;
    let dest = unique_destination(&project.folder_path, &child_file.file_name);
    move_file(&child_file.abs_path, &dest)?;
    queries::delete_child_file(conn, child_file.id)?;
    Ok(dest)
}

pub fn move_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    if std::fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dest)?;
    std::fs::remove_file(src)?;
    Ok(())
}

pub fn unique_destination(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = Path::new(file_name).extension().map(|e| e.to_string_lossy().to_string());

    for n in 2.. {
        let name = match &ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}
