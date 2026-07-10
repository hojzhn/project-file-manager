use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use matr_project_file_manager::db::{queries, Db};
use matr_project_file_manager::model::ExtensionRules;
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
fn settings_round_trip_relevant_directories_and_extension_rules() {
    let db_path = tempdir("settings_db").join("index.sqlite3");
    let db = Db::open(&db_path).unwrap();

    let root = tempdir("settings_root");
    let dir_a = tempdir("settings_dir_a");
    let dir_b = tempdir("settings_dir_b");

    queries::set_root_directory(&db.conn, &root).unwrap();
    queries::replace_relevant_directories(&db.conn, &[dir_a.clone(), dir_b.clone()]).unwrap();
    let rules = ExtensionRules {
        parent_extensions: vec!["svg".to_string(), "png".to_string()],
        child_extensions: vec!["gcode".to_string()],
    };
    queries::set_extension_rules(&db.conn, &rules).unwrap();

    let settings = queries::get_settings(&db.conn).unwrap();
    assert_eq!(settings.root_directory.as_deref(), Some(root.as_path()));
    assert_eq!(settings.relevant_directories, vec![dir_a, dir_b]);
    assert_eq!(settings.extension_rules.parent_extensions, vec!["svg", "png"]);
    assert_eq!(settings.extension_rules.child_extensions, vec!["gcode"]);
}

#[test]
fn multiple_relevant_directories_are_all_scanned_and_matched() {
    let root = tempdir("multi_dir_root");
    let rip_a = tempdir("multi_dir_a");
    let rip_b = tempdir("multi_dir_b");
    let db_path = tempdir("multi_dir_db").join("index.sqlite3");
    let mut db = Db::open(&db_path).unwrap();

    let project_folder = root.join("photo");
    fs::create_dir_all(&project_folder).unwrap();
    fs::write(project_folder.join("photo.jpg"), "x").unwrap();
    let project =
        queries::create_project(&db.conn, "photo", &project_folder, Some("photo.jpg"), Some("photo")).unwrap();
    scanner::scan_project_home(&mut db.conn, &project, &ExtensionRules::default()).unwrap();

    fs::write(rip_a.join("photo-0.prt"), "job").unwrap();
    fs::write(rip_b.join("photo-1.prt"), "job").unwrap();
    scanner::scan_relevant_directories(&mut db.conn, &[rip_a, rip_b], &ExtensionRules::default()).unwrap();

    let files = queries::files_for_project(&db.conn, project.id).unwrap();
    assert!(files.iter().any(|f| f.file_name == "photo-0.prt"), "output from the first folder must match");
    assert!(files.iter().any(|f| f.file_name == "photo-1.prt"), "output from the second folder must match too");
}

#[test]
fn extension_rules_are_configurable_beyond_the_image_prt_bmp_default() {
    let root = tempdir("custom_ext_root");
    let rip_dir = tempdir("custom_ext_rip");
    let db_path = tempdir("custom_ext_db").join("index.sqlite3");
    let mut db = Db::open(&db_path).unwrap();

    let rules = ExtensionRules {
        parent_extensions: vec!["svg".to_string()],
        child_extensions: vec!["gcode".to_string()],
    };

    let project_folder = root.join("design");
    fs::create_dir_all(&project_folder).unwrap();
    fs::write(project_folder.join("design.svg"), "x").unwrap();
    let project =
        queries::create_project(&db.conn, "design", &project_folder, Some("design.svg"), Some("design")).unwrap();
    scanner::scan_project_home(&mut db.conn, &project, &rules).unwrap();

    fs::write(rip_dir.join("design-0.gcode"), "toolpath").unwrap();
    fs::write(rip_dir.join("design-0.jpg"), "not tracked").unwrap();
    scanner::scan_relevant_directories(&mut db.conn, &[rip_dir], &rules).unwrap();

    let files = queries::files_for_project(&db.conn, project.id).unwrap();
    assert!(files.iter().any(|f| f.file_name == "design-0.gcode"), "gcode output must match under custom rules");
    assert!(
        !files.iter().any(|f| f.file_name == "design-0.jpg"),
        "jpg isn't a configured child extension, so it must not be tracked as iteration output"
    );
}
