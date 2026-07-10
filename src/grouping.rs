use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::model::{ExtensionRules, FileSource, ProjectFileView};
use crate::scanner::base_name_of;

pub struct Iteration {
    pub label: String,
    pub files: Vec<ProjectFileView>,
}

pub struct ImageGroup {
    pub image: ProjectFileView,
    pub iterations: Vec<Iteration>,
}

fn file_stem(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string())
}

pub fn group_files(
    files: &[ProjectFileView],
    ext: &ExtensionRules,
) -> (Vec<ImageGroup>, Vec<Iteration>, Vec<ProjectFileView>) {
    let mut images: Vec<ProjectFileView> = files
        .iter()
        .filter(|f| f.source == FileSource::Home && f.base_name.is_some())
        .cloned()
        .collect();
    images.sort_by(|a, b| a.file_name.cmp(&b.file_name));

    let image_names: BTreeSet<String> = images.iter().map(|f| f.file_name.clone()).collect();

    let mut groups: BTreeMap<String, Iteration> = BTreeMap::new();
    let mut leftovers = Vec::new();

    for f in files {
        if image_names.contains(&f.file_name) {
            continue;
        }
        if ext.is_child(&f.ext) {
            let stem = file_stem(&f.file_name);
            let entry = groups.entry(stem.clone()).or_insert_with(|| Iteration { label: stem, files: Vec::new() });
            entry.files.push(f.clone());
        } else {
            leftovers.push(f.clone());
        }
    }
    for iteration in groups.values_mut() {
        iteration.files.sort_by(|a, b| a.ext.cmp(&b.ext));
    }

    let mut image_groups: Vec<ImageGroup> = images
        .into_iter()
        .map(|image| ImageGroup { image, iterations: Vec::new() })
        .collect();

    let mut unassigned = Vec::new();
    for (stem, iteration) in groups {
        let base = base_name_of(&stem);
        match image_groups.iter_mut().find(|g| g.image.base_name.as_deref() == Some(base.as_str())) {
            Some(group) => group.iterations.push(iteration),
            None => unassigned.push(iteration),
        }
    }

    (image_groups, unassigned, leftovers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;

    fn file(source: FileSource, name: &str, ext: &str, base_name: Option<&str>) -> ProjectFileView {
        ProjectFileView {
            source,
            child_file_id: None,
            abs_path: PathBuf::from(name),
            file_name: name.to_string(),
            ext: ext.to_string(),
            size_bytes: 0,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            base_name: base_name.map(str::to_string),
        }
    }

    #[test]
    fn iterations_are_grouped_under_the_image_sharing_their_base_name() {
        let files = vec![
            file(FileSource::Home, "front.jpg", "jpg", Some("front")),
            file(FileSource::Home, "back.jpg", "jpg", Some("back")),
            file(FileSource::Child, "front-0.prt", "prt", Some("front")),
            file(FileSource::Child, "back-0.prt", "prt", Some("back")),
            file(FileSource::Home, "notes.txt", "txt", None),
        ];

        let (image_groups, unassigned, leftovers) = group_files(&files, &ExtensionRules::default());

        assert_eq!(image_groups.len(), 2);
        let front = image_groups.iter().find(|g| g.image.file_name == "front.jpg").unwrap();
        assert_eq!(front.iterations.len(), 1);
        assert_eq!(front.iterations[0].files[0].file_name, "front-0.prt");
        let back = image_groups.iter().find(|g| g.image.file_name == "back.jpg").unwrap();
        assert_eq!(back.iterations.len(), 1);

        assert!(unassigned.is_empty());
        assert_eq!(leftovers.len(), 1);
        assert_eq!(leftovers[0].file_name, "notes.txt");
    }

    #[test]
    fn iterations_with_no_matching_image_are_unassigned() {
        let files = vec![file(FileSource::Child, "orphan-0.prt", "prt", Some("orphan"))];

        let (image_groups, unassigned, _leftovers) = group_files(&files, &ExtensionRules::default());

        assert!(image_groups.is_empty());
        assert_eq!(unassigned.len(), 1);
    }
}
