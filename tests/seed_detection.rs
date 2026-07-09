use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use matr_project_file_manager::db::{queries, Db};
use matr_project_file_manager::scanner;

fn tempdir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "matr_test_{label}_{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn adoption_prefers_real_photo_over_paired_rip_thumbnail() {
    let root = tempdir("root");
    let db_path = tempdir("db").join("index.sqlite3");
    let mut db = Db::open(&db_path).unwrap();

    let folder = root.join("2026-07-09 IMG_4672 copy");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("IMG_4672 copy.jpg"), "photo").unwrap();
    fs::write(folder.join("IMG_4672 copy-0.prt"), "job").unwrap();
    fs::write(folder.join("IMG_4672 copy-0.bmp"), "thumb").unwrap();

    scanner::sync_root_directory(&mut db.conn, &root).unwrap();

    let project = queries::list_projects(&db.conn).unwrap().into_iter().next().unwrap();
    assert_eq!(project.seed_filename.as_deref(), Some("IMG_4672 copy.jpg"));
    assert_eq!(project.seed_basename.as_deref(), Some("img_4672 copy"));
}

#[test]
fn already_broken_seed_self_heals_on_next_sync() {
    let root = tempdir("root2");
    let db_path = tempdir("db2").join("index.sqlite3");
    let mut db = Db::open(&db_path).unwrap();

    let folder = root.join("mug");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("mug.jpg"), "photo").unwrap();
    fs::write(folder.join("mug-0.prt"), "job").unwrap();
    fs::write(folder.join("mug-0.bmp"), "thumb").unwrap();

    let project = queries::create_project(&db.conn, "mug", &folder, Some("mug-0.bmp"), Some("mug")).unwrap();
    assert_eq!(project.seed_filename.as_deref(), Some("mug-0.bmp"));

    scanner::sync_root_directory(&mut db.conn, &root).unwrap();

    let healed = queries::get_project(&db.conn, project.id).unwrap().unwrap();
    assert_eq!(healed.seed_filename.as_deref(), Some("mug.jpg"), "self-healed to the real photo");
}
