use matr_project_file_manager::paths;

#[test]
fn date_prefix_is_yyyy_mm_dd_and_leaves_seed_name_untouched() {
    let prefixed = paths::with_date_prefix("photo");
    let today = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
    assert_eq!(prefixed, format!("{today} photo"));

    assert_eq!(matr_project_file_manager::scanner::base_name_of("photo"), "photo");
}
