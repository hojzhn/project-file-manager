use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::db::queries::{self, ScannedFile, ScannedRipFile};
use crate::error::AppResult;
use crate::model::{ExtensionRules, Project};

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

pub fn scan_project_home(conn: &mut Connection, project: &Project, ext: &ExtensionRules) -> AppResult<usize> {
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
        let file_ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        scanned.push(ScannedFile {
            abs_path: path.to_path_buf(),
            relative_path,
            file_name,
            ext: file_ext,
            size_bytes: metadata.len(),
            created_at: system_time_to_utc(metadata.created()),
            modified_at: system_time_to_utc(metadata.modified()),
            base_name: None,
        });
    }

    let child_only_stems: HashSet<String> = scanned
        .iter()
        .filter(|f| ext.is_child_only(&f.ext))
        .map(|f| file_stem_lower(&f.file_name))
        .collect();
    for f in &mut scanned {
        f.base_name = parent_base_name(&f.file_name, &f.ext, ext, &child_only_stems);
    }

    let count = scanned.len();
    queries::reconcile_home_files(conn, project.id, &scanned)?;
    Ok(count)
}

fn file_stem_lower(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn parent_base_name(
    file_name: &str,
    file_ext: &str,
    ext: &ExtensionRules,
    child_only_stems: &HashSet<String>,
) -> Option<String> {
    if !ext.is_parent(file_ext) {
        return None;
    }
    let stem_lower = file_stem_lower(file_name);
    if ext.is_child(file_ext) && child_only_stems.contains(&stem_lower) {
        return None;
    }
    Some(base_name_of(&stem_lower))
}

pub fn scan_relevant_directories(conn: &mut Connection, dirs: &[PathBuf], ext: &ExtensionRules) -> AppResult<usize> {
    let mut scanned = Vec::new();
    for dir in dirs {
        collect_relevant_files(dir, ext, &mut scanned);
    }

    let count = scanned.len();
    queries::reconcile_rip_files(conn, &scanned)?;
    queries::rematch_rip_files(conn)?;
    Ok(count)
}

fn collect_relevant_files(dir: &Path, ext: &ExtensionRules, scanned: &mut Vec<ScannedRipFile>) {
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let file_ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if !ext.is_child(&file_ext) {
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
            ext: file_ext,
            size_bytes: metadata.len(),
            created_at: system_time_to_utc(metadata.created()),
            modified_at: system_time_to_utc(metadata.modified()),
        });
    }
}

pub fn sync_root_directory(conn: &mut Connection, root: &Path, ext: &ExtensionRules) -> AppResult<usize> {
    let known: HashSet<PathBuf> = queries::list_projects(conn)?.into_iter().map(|p| p.folder_path).collect();

    let mut discovered = 0;
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || known.contains(&path) {
                continue;
            }
            adopt_folder_as_project(conn, &path, ext)?;
            discovered += 1;
        }
    }

    for project in queries::list_projects(conn)? {
        if !project.folder_path.is_dir() {
            queries::archive_project(conn, project.id)?;
            continue;
        }
        heal_misdetected_seed(conn, &project, ext)?;
        scan_project_home(conn, &project, ext)?;
    }
    if discovered > 0 {
        queries::rematch_rip_files(conn)?;
    }

    Ok(discovered)
}

fn adopt_folder_as_project(conn: &mut Connection, folder_path: &Path, ext: &ExtensionRules) -> AppResult<()> {
    let name = folder_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "untitled".to_string());

    let entries = top_level_files(folder_path);
    let (seed_filename, seed_basename) = seed_from_entries(&entries, ext);

    queries::create_project(conn, &name, folder_path, seed_filename.as_deref(), seed_basename.as_deref())?;
    Ok(())
}

fn heal_misdetected_seed(conn: &Connection, project: &Project, ext: &ExtensionRules) -> AppResult<()> {
    let Some(seed_name) = &project.seed_filename else {
        return Ok(());
    };
    let entries = top_level_files(&project.folder_path);
    let seed_path = project.folder_path.join(seed_name);
    if !is_child_masquerading_as_parent(&seed_path, &entries, ext) {
        return Ok(());
    }

    let (seed_filename, seed_basename) = seed_from_entries(&entries, ext);
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

fn seed_from_entries(entries: &[PathBuf], ext: &ExtensionRules) -> (Option<String>, Option<String>) {
    let best = entries
        .iter()
        .filter(|p| is_parent_ext_path(p, ext) && !is_child_masquerading_as_parent(p, entries, ext))
        .min_by_key(|p| {
            let ambiguous = path_ext(p).map(|e| ext.is_child(&e)).unwrap_or(false);
            u8::from(ambiguous)
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

fn path_ext(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase())
}

fn is_child_masquerading_as_parent(path: &Path, siblings: &[PathBuf], ext: &ExtensionRules) -> bool {
    let Some(this_ext) = path_ext(path) else { return false };
    if !(ext.is_parent(&this_ext) && ext.is_child(&this_ext)) {
        return false;
    }
    let stem = path.file_stem();
    siblings.iter().any(|s| {
        s.file_stem() == stem && path_ext(s).map(|e| ext.is_child_only(&e)).unwrap_or(false)
    })
}

fn is_parent_ext_path(path: &Path, ext: &ExtensionRules) -> bool {
    path_ext(path).map(|e| ext.is_parent(&e)).unwrap_or(false)
}
