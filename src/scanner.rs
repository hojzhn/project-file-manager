use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::db::queries::{self, ScannedFile, ScannedRipFile};
use crate::error::AppResult;
use crate::model::{Project, IMAGE_EXTENSIONS, RIP_EXTENSIONS};

fn system_time_to_utc(t: std::io::Result<std::time::SystemTime>) -> DateTime<Utc> {
    t.ok().map(DateTime::<Utc>::from).unwrap_or_else(Utc::now)
}

pub fn base_name_of(file_stem: &str) -> String {
    let lower = file_stem.to_ascii_lowercase();
    if let Some(dash_pos) = lower.rfind('-') {
        let suffix = &lower[dash_pos + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return lower[..dash_pos].to_string();
        }
    }
    lower
}

pub fn scan_project_home(conn: &mut Connection, project: &Project) -> AppResult<usize> {
    let root = &project.folder_path;
    let mut scanned = Vec::new();

    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        scanned.push(ScannedFile {
            abs_path: path.to_path_buf(),
            relative_path,
            file_name,
            ext,
            size_bytes: metadata.len(),
            created_at: system_time_to_utc(metadata.created()),
            modified_at: system_time_to_utc(metadata.modified()),
        });
    }

    let count = scanned.len();
    queries::reconcile_home_files(conn, project.id, &scanned)?;
    Ok(count)
}

pub fn scan_rip_directory(conn: &mut Connection, rip_dir: &Path) -> AppResult<usize> {
    let mut scanned = Vec::new();

    for entry in walkdir::WalkDir::new(rip_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if !RIP_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        scanned.push(ScannedRipFile {
            abs_path: path.to_path_buf(),
            file_name,
            base_name: base_name_of(&stem),
            ext,
            size_bytes: metadata.len(),
            created_at: system_time_to_utc(metadata.created()),
            modified_at: system_time_to_utc(metadata.modified()),
        });
    }

    let count = scanned.len();
    queries::reconcile_rip_files(conn, &scanned)?;
    queries::rematch_rip_files(conn)?;
    Ok(count)
}

pub fn sync_root_directory(conn: &mut Connection, root: &Path) -> AppResult<usize> {
    let known: HashSet<PathBuf> = queries::list_projects(conn)?.into_iter().map(|p| p.folder_path).collect();

    let mut discovered = 0;
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || known.contains(&path) {
                continue;
            }
            adopt_folder_as_project(conn, &path)?;
            discovered += 1;
        }
    }

    for project in queries::list_projects(conn)? {
        if !project.folder_path.is_dir() {
            queries::archive_project(conn, project.id)?;
            continue;
        }
        heal_misdetected_seed(conn, &project)?;
        scan_project_home(conn, &project)?;
    }
    if discovered > 0 {
        queries::rematch_rip_files(conn)?;
    }

    Ok(discovered)
}

fn adopt_folder_as_project(conn: &mut Connection, folder_path: &Path) -> AppResult<()> {
    let name = folder_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "untitled".to_string());

    let entries = top_level_files(folder_path);
    let (seed_filename, seed_basename) = seed_from_entries(&entries);

    queries::create_project(conn, &name, folder_path, seed_filename.as_deref(), seed_basename.as_deref())?;
    Ok(())
}

fn heal_misdetected_seed(conn: &Connection, project: &Project) -> AppResult<()> {
    let Some(seed_name) = &project.seed_filename else {
        return Ok(());
    };
    let entries = top_level_files(&project.folder_path);
    let seed_path = project.folder_path.join(seed_name);
    if !is_rip_thumbnail_bmp(&seed_path, &entries) {
        return Ok(());
    }

    let (seed_filename, seed_basename) = seed_from_entries(&entries);
    queries::update_project_seed(conn, project.id, seed_filename.as_deref(), seed_basename.as_deref())?;
    Ok(())
}

fn top_level_files(folder_path: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(folder_path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect()
}

fn seed_from_entries(entries: &[PathBuf]) -> (Option<String>, Option<String>) {
    let best = entries
        .iter()
        .filter(|p| is_image_extension(p) && !is_rip_thumbnail_bmp(p, entries))
        .min_by_key(|p| {
            let is_bmp = p.extension().map(|e| e.eq_ignore_ascii_case("bmp")).unwrap_or(false);
            u8::from(is_bmp)
        });

    match best {
        Some(path) => {
            let file_name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            (Some(file_name), Some(base_name_of(&stem)))
        }
        None => (None, None),
    }
}

fn is_rip_thumbnail_bmp(path: &Path, siblings: &[PathBuf]) -> bool {
    if !path.extension().map(|e| e.eq_ignore_ascii_case("bmp")).unwrap_or(false) {
        return false;
    }
    let stem = path.file_stem();
    siblings
        .iter()
        .any(|s| s.file_stem() == stem && s.extension().map(|e| e.eq_ignore_ascii_case("prt")).unwrap_or(false))
}

fn is_image_extension(path: &Path) -> bool {
    path.extension()
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
