use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::db::queries;
use crate::error::AppResult;

pub fn canonicalize(path: &Path) -> AppResult<PathBuf> {
    Ok(dunce::canonicalize(path)?)
}

pub fn sanitize_filename(name: &str) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    while matches!(cleaned.chars().last(), Some('.') | Some(' ')) {
        cleaned.pop();
    }
    if cleaned.is_empty() {
        cleaned = "untitled".to_string();
    }
    cleaned
}

pub fn unique_project_folder(conn: &Connection, root: &Path, name: &str) -> AppResult<PathBuf> {
    let base = sanitize_filename(name);
    let mut candidate = root.join(&base);
    let mut n = 2;
    while queries::folder_path_taken(conn, &candidate)? || candidate.exists() {
        candidate = root.join(format!("{base} ({n})"));
        n += 1;
    }
    Ok(candidate)
}

pub fn today_prefix() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

pub fn with_date_prefix(name: &str) -> String {
    format!("{} {name}", today_prefix())
}
