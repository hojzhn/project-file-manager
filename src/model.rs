use std::path::PathBuf;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HomeFileId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RipFileId(pub i64);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub root_directory: Option<PathBuf>,
    pub rip_directory: Option<PathBuf>,
}

impl Settings {
    pub fn is_configured(&self) -> bool {
        self.root_directory.is_some() && self.rip_directory.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub folder_path: PathBuf,
    pub seed_filename: Option<String>,
    pub seed_basename: Option<String>,
    pub created_at: DateTime<Utc>,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeFile {
    pub id: HomeFileId,
    pub project_id: ProjectId,
    pub abs_path: PathBuf,
    pub relative_path: String,
    pub file_name: String,
    pub ext: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub missing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RipFile {
    pub id: RipFileId,
    pub abs_path: PathBuf,
    pub file_name: String,
    pub base_name: String,
    pub ext: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub missing: bool,
    pub matched_project_id: Option<ProjectId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSource {
    Home,
    Rip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFileView {
    pub source: FileSource,
    pub rip_file_id: Option<RipFileId>,
    pub abs_path: PathBuf,
    pub file_name: String,
    pub ext: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Image,
    Vector,
    RipJob,
    Toolpath,
    Other,
}

impl FileKind {
    pub fn from_ext(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "bmp" | "png" | "jpg" | "jpeg" => FileKind::Image,
            "svg" => FileKind::Vector,
            "prt" => FileKind::RipJob,
            "gcode" | "nc" => FileKind::Toolpath,
            _ => FileKind::Other,
        }
    }

    pub fn is_image(self) -> bool {
        matches!(self, FileKind::Image)
    }
}

pub const IMAGE_EXTENSIONS: &[&str] = &["bmp", "png", "jpg", "jpeg"];

pub const RIP_EXTENSIONS: &[&str] = &["prt", "bmp"];
