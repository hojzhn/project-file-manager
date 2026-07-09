use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::model::{
    FileSource, HomeFile, HomeFileId, Project, ProjectFileView, ProjectId, RipFile, RipFileId,
    Settings,
};

fn parse_dt(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn opt_path(s: Option<String>) -> Option<PathBuf> {
    s.map(PathBuf::from)
}

pub fn get_settings(conn: &Connection) -> AppResult<Settings> {
    conn.query_row(
        "SELECT root_directory, rip_directory FROM settings WHERE id = 1",
        [],
        |row| {
            Ok(Settings {
                root_directory: opt_path(row.get(0)?),
                rip_directory: opt_path(row.get(1)?),
            })
        },
    )
    .map_err(AppError::from)
}

pub fn set_directories(conn: &Connection, root: &Path, rip: &Path) -> AppResult<Settings> {
    conn.execute(
        "UPDATE settings SET root_directory = ?1, rip_directory = ?2 WHERE id = 1",
        params![root.to_string_lossy(), rip.to_string_lossy()],
    )?;
    get_settings(conn)
}

fn project_from_row(row: &Row) -> rusqlite::Result<Project> {
    let created_at: String = row.get("created_at")?;
    let folder_path: String = row.get("folder_path")?;
    Ok(Project {
        id: ProjectId(row.get("id")?),
        name: row.get("name")?,
        folder_path: PathBuf::from(folder_path),
        seed_filename: row.get("seed_filename")?,
        seed_basename: row.get("seed_basename")?,
        created_at: parse_dt(&created_at),
        archived: row.get::<_, i64>("archived")? != 0,
    })
}

pub fn create_project(
    conn: &Connection,
    name: &str,
    folder_path: &Path,
    seed_filename: Option<&str>,
    seed_basename: Option<&str>,
) -> AppResult<Project> {
    let now = Utc::now();
    conn.execute(
        "INSERT INTO projects (name, folder_path, seed_filename, seed_basename, created_at, archived)
         VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        params![
            name,
            folder_path.to_string_lossy(),
            seed_filename,
            seed_basename,
            now.to_rfc3339(),
        ],
    )?;
    let id = conn.last_insert_rowid();
    get_project(conn, ProjectId(id))?.ok_or(AppError::ProjectNotFound(ProjectId(id)))
}

pub fn get_project(conn: &Connection, id: ProjectId) -> AppResult<Option<Project>> {
    conn.query_row("SELECT * FROM projects WHERE id = ?1", params![id.0], project_from_row)
        .optional()
        .map_err(AppError::from)
}

pub fn list_projects(conn: &Connection) -> AppResult<Vec<Project>> {
    let mut stmt = conn.prepare("SELECT * FROM projects WHERE archived = 0 ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], project_from_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
}

pub fn update_project_seed(
    conn: &Connection,
    project_id: ProjectId,
    seed_filename: Option<&str>,
    seed_basename: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE projects SET seed_filename = ?1, seed_basename = ?2 WHERE id = ?3",
        params![seed_filename, seed_basename, project_id.0],
    )?;
    Ok(())
}

pub fn archive_project(conn: &Connection, project_id: ProjectId) -> AppResult<()> {
    conn.execute("UPDATE projects SET archived = 1 WHERE id = ?1", params![project_id.0])?;
    conn.execute(
        "UPDATE rip_files SET matched_project_id = NULL WHERE matched_project_id = ?1",
        params![project_id.0],
    )?;
    Ok(())
}

pub fn folder_path_taken(conn: &Connection, folder_path: &Path) -> AppResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM projects WHERE folder_path = ?1",
        params![folder_path.to_string_lossy()],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn home_file_from_row(row: &Row) -> rusqlite::Result<HomeFile> {
    let abs_path: String = row.get("abs_path")?;
    let created_at: String = row.get("created_at")?;
    let modified_at: String = row.get("modified_at")?;
    Ok(HomeFile {
        id: HomeFileId(row.get("id")?),
        project_id: ProjectId(row.get("project_id")?),
        abs_path: PathBuf::from(abs_path),
        relative_path: row.get("relative_path")?,
        file_name: row.get("file_name")?,
        ext: row.get("ext")?,
        size_bytes: row.get::<_, i64>("size_bytes")? as u64,
        created_at: parse_dt(&created_at),
        modified_at: parse_dt(&modified_at),
        missing: row.get::<_, i64>("missing")? != 0,
    })
}

pub struct ScannedFile {
    pub abs_path: PathBuf,
    pub relative_path: String,
    pub file_name: String,
    pub ext: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

pub fn reconcile_home_files(
    conn: &mut Connection,
    project_id: ProjectId,
    scanned: &[ScannedFile],
) -> AppResult<()> {
    let tx = conn.transaction()?;
    let now = Utc::now().to_rfc3339();

    {
        let mut upsert = tx.prepare(
            "INSERT INTO home_files (project_id, abs_path, relative_path, file_name, ext,
                                      size_bytes, created_at, modified_at, missing, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)
             ON CONFLICT(abs_path) DO UPDATE SET
                project_id = excluded.project_id,
                relative_path = excluded.relative_path,
                file_name = excluded.file_name,
                ext = excluded.ext,
                size_bytes = excluded.size_bytes,
                created_at = excluded.created_at,
                modified_at = excluded.modified_at,
                missing = 0,
                last_seen_at = excluded.last_seen_at",
        )?;
        for f in scanned {
            upsert.execute(params![
                project_id.0,
                f.abs_path.to_string_lossy(),
                f.relative_path,
                f.file_name,
                f.ext,
                f.size_bytes as i64,
                f.created_at.to_rfc3339(),
                f.modified_at.to_rfc3339(),
                now,
            ])?;
        }
    }

    tx.execute(
        "UPDATE home_files SET missing = 1 WHERE project_id = ?1 AND last_seen_at != ?2",
        params![project_id.0, now],
    )?;

    tx.commit()?;
    Ok(())
}

fn rip_file_from_row(row: &Row) -> rusqlite::Result<RipFile> {
    let abs_path: String = row.get("abs_path")?;
    let created_at: String = row.get("created_at")?;
    let modified_at: String = row.get("modified_at")?;
    let matched_project_id: Option<i64> = row.get("matched_project_id")?;
    Ok(RipFile {
        id: RipFileId(row.get("id")?),
        abs_path: PathBuf::from(abs_path),
        file_name: row.get("file_name")?,
        base_name: row.get("base_name")?,
        ext: row.get("ext")?,
        size_bytes: row.get::<_, i64>("size_bytes")? as u64,
        created_at: parse_dt(&created_at),
        modified_at: parse_dt(&modified_at),
        missing: row.get::<_, i64>("missing")? != 0,
        matched_project_id: matched_project_id.map(ProjectId),
    })
}

pub struct ScannedRipFile {
    pub abs_path: PathBuf,
    pub file_name: String,
    pub base_name: String,
    pub ext: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

pub fn reconcile_rip_files(conn: &mut Connection, scanned: &[ScannedRipFile]) -> AppResult<()> {
    let tx = conn.transaction()?;
    let now = Utc::now().to_rfc3339();

    {
        let mut upsert = tx.prepare(
            "INSERT INTO rip_files (abs_path, file_name, base_name, ext, size_bytes,
                                     created_at, modified_at, missing, matched_project_id, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, NULL, ?8)
             ON CONFLICT(abs_path) DO UPDATE SET
                file_name = excluded.file_name,
                base_name = excluded.base_name,
                ext = excluded.ext,
                size_bytes = excluded.size_bytes,
                created_at = excluded.created_at,
                modified_at = excluded.modified_at,
                missing = 0,
                last_seen_at = excluded.last_seen_at",
        )?;
        for f in scanned {
            upsert.execute(params![
                f.abs_path.to_string_lossy(),
                f.file_name,
                f.base_name,
                f.ext,
                f.size_bytes as i64,
                f.created_at.to_rfc3339(),
                f.modified_at.to_rfc3339(),
                now,
            ])?;
        }
    }

    tx.execute(
        "UPDATE rip_files SET missing = 1 WHERE last_seen_at != ?1",
        params![now],
    )?;

    tx.commit()?;
    Ok(())
}

pub fn rematch_rip_files(conn: &Connection) -> AppResult<usize> {
    conn.execute(
        "UPDATE rip_files
         SET matched_project_id = (
             SELECT p.id FROM projects p
             WHERE p.seed_basename = rip_files.base_name AND p.archived = 0
             ORDER BY p.created_at DESC
             LIMIT 1
         )
         WHERE matched_project_id IS NULL
           AND EXISTS (
             SELECT 1 FROM projects p
             WHERE p.seed_basename = rip_files.base_name AND p.archived = 0
           )",
        [],
    )
    .map_err(AppError::from)
}

pub fn get_rip_file(conn: &Connection, id: RipFileId) -> AppResult<Option<RipFile>> {
    conn.query_row("SELECT * FROM rip_files WHERE id = ?1", params![id.0], rip_file_from_row)
        .optional()
        .map_err(AppError::from)
}

pub fn rip_files_matched_to_project(conn: &Connection, project_id: ProjectId) -> AppResult<Vec<RipFile>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM rip_files WHERE matched_project_id = ?1 AND missing = 0 ORDER BY file_name",
    )?;
    let rows = stmt.query_map(params![project_id.0], rip_file_from_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
}

pub fn delete_rip_file(conn: &Connection, id: RipFileId) -> AppResult<()> {
    conn.execute("DELETE FROM rip_files WHERE id = ?1", params![id.0])?;
    Ok(())
}

pub fn files_for_project(conn: &Connection, project_id: ProjectId) -> AppResult<Vec<ProjectFileView>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM home_files WHERE project_id = ?1 AND missing = 0 ORDER BY file_name",
    )?;
    let home_rows = stmt.query_map(params![project_id.0], home_file_from_row)?;
    let mut views: Vec<ProjectFileView> = home_rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|f| ProjectFileView {
            source: FileSource::Home,
            rip_file_id: None,
            abs_path: f.abs_path,
            file_name: f.file_name,
            ext: f.ext,
            size_bytes: f.size_bytes,
            created_at: f.created_at,
            modified_at: f.modified_at,
        })
        .collect();

    let rip_files = rip_files_matched_to_project(conn, project_id)?;
    views.extend(rip_files.into_iter().map(|f| ProjectFileView {
        source: FileSource::Rip,
        rip_file_id: Some(f.id),
        abs_path: f.abs_path,
        file_name: f.file_name,
        ext: f.ext,
        size_bytes: f.size_bytes,
        created_at: f.created_at,
        modified_at: f.modified_at,
    }));

    views.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(views)
}
