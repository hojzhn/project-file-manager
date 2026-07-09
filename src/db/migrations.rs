use rusqlite_migration::{Migrations, M};

pub fn runner() -> Migrations<'static> {
    Migrations::new(vec![M::up(SCHEMA_V1)])
}

const SCHEMA_V1: &str = r#"
CREATE TABLE settings (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    root_directory TEXT,
    rip_directory  TEXT
);
INSERT INTO settings (id, root_directory, rip_directory) VALUES (1, NULL, NULL);

CREATE TABLE projects (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL,
    folder_path    TEXT NOT NULL UNIQUE,
    seed_filename  TEXT,
    seed_basename  TEXT,
    created_at     TEXT NOT NULL,
    archived       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_projects_seed_basename ON projects(seed_basename);

CREATE TABLE home_files (
    id            INTEGER PRIMARY KEY,
    project_id    INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    abs_path      TEXT NOT NULL UNIQUE,
    relative_path TEXT NOT NULL,
    file_name     TEXT NOT NULL,
    ext           TEXT NOT NULL,
    size_bytes    INTEGER NOT NULL,
    created_at    TEXT NOT NULL,
    modified_at   TEXT NOT NULL,
    missing       INTEGER NOT NULL DEFAULT 0,
    last_seen_at  TEXT NOT NULL
);
CREATE INDEX idx_home_files_project ON home_files(project_id, missing);

CREATE TABLE rip_files (
    id                 INTEGER PRIMARY KEY,
    abs_path           TEXT NOT NULL UNIQUE,
    file_name          TEXT NOT NULL,
    base_name          TEXT NOT NULL,
    ext                TEXT NOT NULL,
    size_bytes         INTEGER NOT NULL,
    created_at         TEXT NOT NULL,
    modified_at        TEXT NOT NULL,
    missing            INTEGER NOT NULL DEFAULT 0,
    matched_project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    last_seen_at       TEXT NOT NULL
);
CREATE INDEX idx_rip_files_project ON rip_files(matched_project_id, missing);
CREATE INDEX idx_rip_files_base_name ON rip_files(base_name);
"#;
