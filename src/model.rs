use std::path::PathBuf;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HomeFileId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChildFileId(pub i64);

pub const DEFAULT_PARENT_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp"];
pub const DEFAULT_CHILD_EXTENSIONS: &[&str] = &["prt", "bmp"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionRules {
    pub parent_extensions: Vec<String>,
    pub child_extensions: Vec<String>,
}

impl Default for ExtensionRules {
    fn default() -> Self {
        Self {
            parent_extensions: DEFAULT_PARENT_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
            child_extensions: DEFAULT_CHILD_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl ExtensionRules {
    pub fn is_parent(&self, ext: &str) -> bool {
        self.parent_extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }

    pub fn is_child(&self, ext: &str) -> bool {
        self.child_extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }

    pub fn is_child_only(&self, ext: &str) -> bool {
        self.is_child(ext) && !self.is_parent(ext)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub root_directory: Option<PathBuf>,
    pub relevant_directories: Vec<PathBuf>,
    pub extension_rules: ExtensionRules,
}

impl Settings {
    pub fn is_configured(&self) -> bool {
        self.root_directory.is_some() && !self.relevant_directories.is_empty()
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
    pub base_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildFile {
    pub id: ChildFileId,
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
    Child,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFileView {
    pub source: FileSource,
    pub child_file_id: Option<ChildFileId>,
    pub abs_path: PathBuf,
    pub file_name: String,
    pub ext: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub base_name: Option<String>,
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
