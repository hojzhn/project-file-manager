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
fn untracked_folders_under_root_are_adopted_as_projects() {
    let root = tempdir("root");
    let db_path = tempdir("db").join("index.sqlite3");
    let mut db = Db::open(&db_path).unwrap();

    let folder_a = root.join("2026-01-01 mug");
    fs::create_dir_all(&folder_a).unwrap();
    fs::write(folder_a.join("mug.png"), "x").unwrap();

    let folder_b = root.join("2026-01-02 plain");
    fs::create_dir_all(&folder_b).unwrap();
    fs::write(folder_b.join("notes.txt"), "x").unwrap();

    let discovered = scanner::sync_root_directory(&mut db.conn, &root).unwrap();
    assert_eq!(discovered, 2);

    let projects = queries::list_projects(&db.conn).unwrap();
    assert_eq!(projects.len(), 2);

    let mug_project = projects.iter().find(|p| p.folder_path == folder_a).unwrap();
    assert_eq!(mug_project.seed_filename.as_deref(), Some("mug.png"), "image inside is opportunistically treated as the seed");
    assert_eq!(mug_project.seed_basename.as_deref(), Some("mug"));

    let plain_project = projects.iter().find(|p| p.folder_path == folder_b).unwrap();
    assert_eq!(plain_project.seed_filename, None, "no image inside, so no seed is inferred");

    let mug_files = queries::files_for_project(&db.conn, mug_project.id).unwrap();
    assert_eq!(mug_files.len(), 1);

    let discovered_again = scanner::sync_root_directory(&mut db.conn, &root).unwrap();
    assert_eq!(discovered_again, 0);
    assert_eq!(queries::list_projects(&db.conn).unwrap().len(), 2);
}

#[test]
fn adopted_folder_seed_matches_rip_output() {
    let root = tempdir("root2");
    let rip_dir = tempdir("rip2");
    let db_path = tempdir("db2").join("index.sqlite3");
    let mut db = Db::open(&db_path).unwrap();

    let folder = root.join("mug");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("mug.png"), "x").unwrap();

    scanner::sync_root_directory(&mut db.conn, &root).unwrap();

    fs::write(rip_dir.join("mug-0.prt"), "job").unwrap();
    scanner::scan_rip_directory(&mut db.conn, &rip_dir).unwrap();

    let project = queries::list_projects(&db.conn).unwrap().into_iter().next().unwrap();
    let files = queries::files_for_project(&db.conn, project.id).unwrap();
    assert!(files.iter().any(|f| f.file_name == "mug-0.prt"));
}

#[test]
fn deleting_a_project_folder_removes_it_from_the_list() {
    let root = tempdir("root3");
    let rip_dir = tempdir("rip3");
    let db_path = tempdir("db3").join("index.sqlite3");
    let mut db = Db::open(&db_path).unwrap();

    let folder = root.join("mug");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("mug.png"), "x").unwrap();
    scanner::sync_root_directory(&mut db.conn, &root).unwrap();

    fs::write(rip_dir.join("mug-0.prt"), "job").unwrap();
    scanner::scan_rip_directory(&mut db.conn, &rip_dir).unwrap();

    let project = queries::list_projects(&db.conn).unwrap().into_iter().next().unwrap();
    let matched_before = queries::rip_files_matched_to_project(&db.conn, project.id).unwrap();
    assert_eq!(matched_before.len(), 1, "mug-0.prt matched before the folder was removed");

    fs::remove_dir_all(&folder).unwrap();
    scanner::sync_root_directory(&mut db.conn, &root).unwrap();

    let projects = queries::list_projects(&db.conn).unwrap();
    assert!(projects.is_empty(), "project must disappear once its folder is gone");

    let rip_file = queries::get_rip_file(&db.conn, matched_before[0].id).unwrap().unwrap();
    assert_eq!(rip_file.matched_project_id, None, "orphaned rip file must be unmatched, not left pointing at a hidden project");
}
