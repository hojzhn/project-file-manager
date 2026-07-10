use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use matr_project_file_manager::db::{queries, Db};
use matr_project_file_manager::model::{ExtensionRules, FileSource};
use matr_project_file_manager::organize;
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
fn base_name_strips_printfactory_duplicate_suffix() {
    assert_eq!(scanner::base_name_of("photo-0"), "photo");
    assert_eq!(scanner::base_name_of("photo-12"), "photo");
    assert_eq!(scanner::base_name_of("photo"), "photo");
    assert_eq!(scanner::base_name_of("PHOTO"), "photo");
    assert_eq!(scanner::base_name_of("my-project"), "my-project");
    assert_eq!(scanner::base_name_of("my-project-2"), "my-project");
}

#[test]
fn child_files_are_matched_by_filename_and_only_moved_on_request() {
    let root = tempdir("root");
    let relevant_dir = tempdir("rip");
    let db_path = tempdir("db").join("index.sqlite3");
    let mut db = Db::open(&db_path).unwrap();

    let project_folder = root.join("photo");
    fs::create_dir_all(&project_folder).unwrap();
    fs::write(project_folder.join("photo.jpg"), "x").unwrap();
    let project =
        queries::create_project(&db.conn, "photo", &project_folder, Some("photo.jpg"), Some("photo")).unwrap();
    scanner::scan_project_home(&mut db.conn, &project, &ExtensionRules::default()).unwrap();

    fs::write(relevant_dir.join("photo-0.prt"), "job").unwrap();
    fs::write(relevant_dir.join("photo-0.bmp"), "thumb").unwrap();
    scanner::scan_relevant_directories(&mut db.conn, &[relevant_dir.clone()], &ExtensionRules::default()).unwrap();

    let files = queries::files_for_project(&db.conn, project.id).unwrap();
    assert_eq!(files.len(), 3, "seed image + matched .prt + matched .bmp");

    let home_count = files.iter().filter(|f| f.source == FileSource::Home).count();
    let child_count = files.iter().filter(|f| f.source == FileSource::Child).count();
    assert_eq!(home_count, 1, "only the seed image lives in the project's own folder");
    assert_eq!(child_count, 2, "the .prt/.bmp are matched but still physically in the relevant directory");

    assert!(relevant_dir.join("photo-0.prt").exists());
    assert!(relevant_dir.join("photo-0.bmp").exists());
    assert!(!project_folder.join("photo-0.prt").exists());

    let child_file = files
        .iter()
        .find(|f| f.file_name == "photo-0.prt")
        .and_then(|f| f.child_file_id)
        .unwrap();
    let child_file = queries::get_child_file(&db.conn, child_file).unwrap().unwrap();
    organize::move_child_file_into_project(&mut db.conn, &child_file, &project).unwrap();
    scanner::scan_project_home(&mut db.conn, &project, &ExtensionRules::default()).unwrap();

    assert!(!relevant_dir.join("photo-0.prt").exists(), "moved out of the relevant directory");
    assert!(project_folder.join("photo-0.prt").exists(), "landed in the project folder");
    assert!(relevant_dir.join("photo-0.bmp").exists(), "the other matched file was untouched");

    let files = queries::files_for_project(&db.conn, project.id).unwrap();
    let moved = files.iter().find(|f| f.file_name == "photo-0.prt").unwrap();
    assert_eq!(moved.source, FileSource::Home, "moved file is now tracked as a home file");
}

#[test]
fn child_output_for_every_image_in_a_project_gets_matched_not_just_the_seed() {
    let root = tempdir("multi_root");
    let relevant_dir = tempdir("multi_rip");
    let db_path = tempdir("multi_db").join("index.sqlite3");
    let mut db = Db::open(&db_path).unwrap();

    let project_folder = root.join("project");
    fs::create_dir_all(&project_folder).unwrap();
    fs::write(project_folder.join("front.jpg"), "x").unwrap();
    let project =
        queries::create_project(&db.conn, "project", &project_folder, Some("front.jpg"), Some("front")).unwrap();
    scanner::scan_project_home(&mut db.conn, &project, &ExtensionRules::default()).unwrap();

    fs::write(project_folder.join("back.jpg"), "y").unwrap();
    scanner::scan_project_home(&mut db.conn, &project, &ExtensionRules::default()).unwrap();

    fs::write(relevant_dir.join("front-0.prt"), "job").unwrap();
    fs::write(relevant_dir.join("back-0.prt"), "job").unwrap();
    scanner::scan_relevant_directories(&mut db.conn, &[relevant_dir.clone()], &ExtensionRules::default()).unwrap();

    let files = queries::files_for_project(&db.conn, project.id).unwrap();
    assert!(
        files.iter().any(|f| f.file_name == "front-0.prt"),
        "child output for the first image must be matched"
    );
    assert!(
        files.iter().any(|f| f.file_name == "back-0.prt"),
        "child output for the second image must be matched too, not just the first/seed image"
    );
}

#[test]
fn unmatched_child_files_stay_unmatched() {
    let relevant_dir = tempdir("rip_unmatched");
    let db_path = tempdir("db2").join("index.sqlite3");
    let mut db = Db::open(&db_path).unwrap();

    fs::write(relevant_dir.join("someone_elses_job-0.prt"), "job").unwrap();
    scanner::scan_relevant_directories(&mut db.conn, &[relevant_dir.clone()], &ExtensionRules::default()).unwrap();

    let root = tempdir("root2");
    let project_folder = root.join("photo");
    fs::create_dir_all(&project_folder).unwrap();
    let project =
        queries::create_project(&db.conn, "photo", &project_folder, Some("photo.jpg"), Some("photo")).unwrap();

    let files = queries::files_for_project(&db.conn, project.id).unwrap();
    assert!(files.is_empty(), "unrelated child file must not attach to this project");
}
