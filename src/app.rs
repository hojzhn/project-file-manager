use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use crossbeam_channel::{Receiver, Sender};
use eframe::egui;

use crate::commands::{Command, UiEvent};
use crate::model::{ExtensionRules, FileSource, Project, ProjectFileView, ProjectId, Settings};
use crate::scanner::base_name_of;

struct Iteration {
    label: String,
    files: Vec<ProjectFileView>,
}

struct ImageGroup {
    image: ProjectFileView,
    iterations: Vec<Iteration>,
}

fn file_stem(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string())
}

fn group_files(
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

struct SetupDialog {
    root: Option<PathBuf>,
    relevant_dirs: Vec<PathBuf>,
    parent_extensions: String,
    child_extensions: String,
}

fn setup_dialog_from_settings(settings: &Settings) -> SetupDialog {
    SetupDialog {
        root: settings.root_directory.clone(),
        relevant_dirs: settings.relevant_directories.clone(),
        parent_extensions: settings.extension_rules.parent_extensions.join(", "),
        child_extensions: settings.extension_rules.child_extensions.join(", "),
    }
}

fn parse_ext_list(s: &str) -> Vec<String> {
    s.split(',').map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase()).filter(|e| !e.is_empty()).collect()
}

struct ScratchDialog {
    name: String,
}

pub struct App {
    command_tx: Sender<Command>,
    ui_event_rx: Receiver<UiEvent>,

    settings: Settings,
    projects: Vec<Project>,
    selected_project: Option<ProjectId>,
    files_by_project: HashMap<ProjectId, Vec<ProjectFileView>>,

    setup_dialog: Option<SetupDialog>,
    new_project_menu_open: bool,
    scratch_dialog: Option<ScratchDialog>,

    thumbnails: HashMap<PathBuf, egui::TextureHandle>,
    pending_thumbnails: HashSet<PathBuf>,
    thumbnail_request_tx: Sender<ThumbnailRequest>,
    thumbnail_result_rx: Receiver<(PathBuf, Option<egui::TextureHandle>)>,

    status: Option<String>,
}

const THUMBNAIL_MAX_DIM: u32 = 160;

struct ThumbnailRequest {
    path: PathBuf,
    modified_unix: i64,
    size_bytes: u64,
}

fn spawn_thumbnail_loader(
    ctx: egui::Context,
    cache_dir: Option<PathBuf>,
) -> (Sender<ThumbnailRequest>, Receiver<(PathBuf, Option<egui::TextureHandle>)>) {
    let (request_tx, request_rx) = crossbeam_channel::unbounded::<ThumbnailRequest>();
    let (result_tx, result_rx) = crossbeam_channel::unbounded::<(PathBuf, Option<egui::TextureHandle>)>();

    let worker_count = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 4);
    for _ in 0..worker_count {
        let request_rx = request_rx.clone();
        let result_tx = result_tx.clone();
        let ctx = ctx.clone();
        let cache_dir = cache_dir.clone();
        std::thread::spawn(move || {
            while let Ok(req) = request_rx.recv() {
                let tex = load_thumbnail_rgba(&req, cache_dir.as_deref()).map(|rgba| {
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                    ctx.load_texture(req.path.to_string_lossy(), color_image, egui::TextureOptions::LINEAR)
                });
                if result_tx.send((req.path, tex)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
    }

    (request_tx, result_rx)
}

fn thumbnail_cache_path(cache_dir: &Path, req: &ThumbnailRequest) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    req.path.hash(&mut hasher);
    let path_hash = hasher.finish();
    cache_dir.join(format!("{path_hash:016x}_{}_{}.png", req.modified_unix, req.size_bytes))
}

fn load_thumbnail_rgba(req: &ThumbnailRequest, cache_dir: Option<&Path>) -> Option<image::RgbaImage> {
    let cache_path = cache_dir.map(|dir| thumbnail_cache_path(dir, req));

    if let Some(cache_path) = &cache_path {
        if let Ok(cached) = image::open(cache_path) {
            return Some(cached.into_rgba8());
        }
    }

    let decoded = image::open(&req.path).ok()?.thumbnail(THUMBNAIL_MAX_DIM, THUMBNAIL_MAX_DIM).into_rgba8();

    if let Some(cache_path) = &cache_path {
        if let Some(dir) = cache_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = decoded.save(cache_path);
    }

    Some(decoded)
}

impl App {
    pub fn new(command_tx: Sender<Command>, ui_event_rx: Receiver<UiEvent>, ctx: egui::Context) -> Self {
        let cache_dir = crate::db::thumbnail_cache_dir().ok();
        let (thumbnail_request_tx, thumbnail_result_rx) = spawn_thumbnail_loader(ctx, cache_dir);
        Self {
            command_tx,
            ui_event_rx,
            settings: Settings::default(),
            projects: Vec::new(),
            selected_project: None,
            files_by_project: HashMap::new(),
            setup_dialog: None,
            new_project_menu_open: false,
            scratch_dialog: None,
            thumbnails: HashMap::new(),
            pending_thumbnails: HashSet::new(),
            thumbnail_request_tx,
            thumbnail_result_rx,
            status: None,
        }
    }

    fn send(&self, cmd: Command) {
        let _ = self.command_tx.send(cmd);
    }

    fn thumbnail_texture(&mut self, file: &ProjectFileView) -> Option<egui::TextureHandle> {
        let path = &file.abs_path;
        if let Some(tex) = self.thumbnails.get(path) {
            return Some(tex.clone());
        }
        if self.pending_thumbnails.insert(path.clone()) {
            let _ = self.thumbnail_request_tx.send(ThumbnailRequest {
                path: path.clone(),
                modified_unix: file.modified_at.timestamp(),
                size_bytes: file.size_bytes,
            });
        }
        None
    }

    fn drain_thumbnail_results(&mut self) {
        while let Ok((path, tex)) = self.thumbnail_result_rx.try_recv() {
            self.pending_thumbnails.remove(&path);
            if let Some(tex) = tex {
                self.thumbnails.insert(path, tex);
            }
        }
    }

    fn show_file_row(&mut self, ui: &mut egui::Ui, file: &ProjectFileView) {
        ui.horizontal(|ui| {
            ui.label(&file.file_name);
            ui.weak(format!("{} bytes", file.size_bytes));
            ui.weak(file.modified_at.format("%Y-%m-%d").to_string());
            if ui.small_button("Open").on_hover_text("Open with the default app for this file type").clicked() {
                if let Err(e) = open::that(&file.abs_path) {
                    self.status = Some(format!("Couldn't open {}: {e}", file.file_name));
                }
            }
            if ui.small_button("Reveal").on_hover_text("Show in File Explorer").clicked() {
                if let Err(e) = reveal_in_explorer(&file.abs_path) {
                    self.status = Some(format!("Couldn't reveal {}: {e}", file.file_name));
                }
            }
            match file.source {
                FileSource::Home => {
                    ui.weak("home");
                }
                FileSource::Rip => {
                    ui.colored_label(egui::Color32::YELLOW, "rip (not moved)");
                    if let Some(rip_file_id) = file.rip_file_id {
                        if ui.small_button("Move").clicked() {
                            self.send(Command::MoveRipFileIntoProject { rip_file_id });
                        }
                    }
                }
            }
        });
    }

    fn show_iteration(&mut self, ui: &mut egui::Ui, iteration: &Iteration) {
        let thumb_file = iteration.files.iter().find(|f| self.settings.extension_rules.is_parent(&f.ext)).cloned();
        let tex = thumb_file.as_ref().and_then(|f| self.thumbnail_texture(f));
        ui.group(|ui| {
            ui.horizontal(|ui| {
                if let Some(tex) = &tex {
                    ui.add(egui::Image::new(tex).max_size(egui::vec2(96.0, 96.0)));
                } else {
                    ui.weak("(no thumbnail yet)");
                }
                ui.vertical(|ui| {
                    ui.label(&iteration.label);
                    for file in &iteration.files {
                        self.show_file_row(ui, file);
                    }
                });
            });
        });
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.ui_event_rx.try_recv() {
            match event {
                UiEvent::Settings(settings) => {
                    if settings.is_configured() {
                        self.setup_dialog = None;
                    } else if self.setup_dialog.is_none() {
                        self.setup_dialog = Some(setup_dialog_from_settings(&settings));
                    }
                    self.settings = settings;
                }
                UiEvent::ProjectsList(projects) => self.projects = projects,
                UiEvent::ProjectCreated(project) => {
                    self.selected_project = Some(project.id);
                    if !self.projects.iter().any(|p| p.id == project.id) {
                        self.projects.push(project);
                    }
                }
                UiEvent::FilesForProject { project_id, files } => {
                    self.files_by_project.insert(project_id, files);
                }
                UiEvent::Error(message) => self.status = Some(message),
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            let Some(path) = file.path else { continue };
            if self.is_source_path(&path) {
                self.send(Command::CreateProjectFromImage { image_path: path });
            } else {
                self.status = Some(format!("Dropped file isn't a recognized image: {}", path.display()));
            }
        }
    }

    fn is_source_path(&self, path: &Path) -> bool {
        path.extension()
            .map(|e| self.settings.extension_rules.is_parent(&e.to_string_lossy().to_ascii_lowercase()))
            .unwrap_or(false)
    }

    fn show_status_bar(&mut self, ui: &mut egui::Ui) {
        if let Some(status) = self.status.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::RED, &status);
                if ui.small_button("dismiss").clicked() {
                    self.status = None;
                }
            });
            ui.separator();
        }
    }

    fn show_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(240.0)
            .show(ui, |ui| {
                ui.heading("Projects");
                if ui.button("+ New Project").clicked() {
                    self.new_project_menu_open = true;
                }
                ui.separator();

                let mut clicked = None;
                for project in &self.projects {
                    let selected = self.selected_project == Some(project.id);
                    if ui.selectable_label(selected, &project.name).clicked() {
                        clicked = Some(project.id);
                    }
                }
                if let Some(id) = clicked {
                    self.selected_project = Some(id);
                    self.send(Command::ListFilesForProject { project_id: id });
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if ui.button("⚙ Directories...").clicked() {
                        self.setup_dialog = Some(setup_dialog_from_settings(&self.settings));
                    }
                    if ui
                        .button("⟳ Sync now")
                        .on_hover_text(
                            "Manually re-check the root and relevant directories. \
                             Changes are picked up live, so this is only needed \
                             as a fallback.",
                        )
                        .clicked()
                    {
                        self.send(Command::SyncRootDirectory);
                        self.send(Command::SyncRelevantDirectories);
                    }
                });
            });
    }

    fn show_new_project_menu(&mut self, ctx: &egui::Context) {
        if !self.new_project_menu_open {
            return;
        }
        let mut open = true;
        let mut close = false;

        egui::Window::new("New Project")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let parent_exts: Vec<&str> =
                    self.settings.extension_rules.parent_extensions.iter().map(String::as_str).collect();
                if ui.button("📥 Select image from Downloads...").clicked() {
                    let mut dialog = rfd::FileDialog::new().add_filter("Image", &parent_exts);
                    if let Some(dir) = downloads_dir() {
                        dialog = dialog.set_directory(dir);
                    }
                    if let Some(path) = dialog.pick_file() {
                        self.send(Command::CreateProjectFromImage { image_path: path });
                    }
                    close = true;
                }
                if ui.button("🖼 Browse for image...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("Image", &parent_exts).pick_file() {
                        self.send(Command::CreateProjectFromImage { image_path: path });
                    }
                    close = true;
                }
                if ui.button("✎ Create from scratch...").clicked() {
                    self.scratch_dialog = Some(ScratchDialog { name: String::new() });
                    close = true;
                }
                ui.separator();
                ui.weak("...or drag and drop an image onto this window.");
            });

        if !open || close {
            self.new_project_menu_open = false;
        }
    }

    fn show_scratch_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.scratch_dialog else {
            return;
        };
        let mut open = true;
        let mut create = false;

        egui::Window::new("New Project from Scratch")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut dialog.name);
                });
                let ready = !dialog.name.trim().is_empty();
                if ui.add_enabled(ready, egui::Button::new("Create")).clicked() {
                    create = true;
                }
            });

        if create {
            if let Some(dialog) = self.scratch_dialog.take() {
                self.send(Command::CreateProjectFromScratch { name: dialog.name.trim().to_string() });
            }
        } else if !open {
            self.scratch_dialog = None;
        }
    }

    fn show_setup_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.setup_dialog else {
            return;
        };
        let mut submit = false;

        egui::Window::new("Set up directories")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(
                    "Choose where project folders are created, and which folder(s) hold \
                     output from external tools (like PrintFactory RIP) that should be \
                     matched into projects by filename.",
                );
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if ui.button("Choose root directory...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            dialog.root = Some(path);
                        }
                    }
                    ui.weak(display_or(&dialog.root, "Not set"));
                });

                ui.add_space(8.0);
                ui.label("Relevant directories (e.g. RIP output folders):");
                let mut remove_at = None;
                for (i, dir) in dialog.relevant_dirs.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("Remove").clicked() {
                            remove_at = Some(i);
                        }
                        ui.label(dir.display().to_string());
                    });
                }
                if let Some(i) = remove_at {
                    dialog.relevant_dirs.remove(i);
                }
                if ui.button("+ Add folder...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        if !dialog.relevant_dirs.contains(&path) {
                            dialog.relevant_dirs.push(path);
                        }
                    }
                }

                ui.add_space(8.0);
                ui.label("Source (\"parent\") file extensions, comma-separated:");
                ui.text_edit_singleline(&mut dialog.parent_extensions);
                ui.label("Iteration (\"child\") output extensions, comma-separated:");
                ui.text_edit_singleline(&mut dialog.child_extensions);
                ui.weak(
                    "An extension in both lists (like the default bmp) is treated as a source \
                     file unless a sibling with a child-only extension (like prt) shares its name.",
                );

                ui.add_space(8.0);
                let ready = dialog.root.is_some() && !dialog.relevant_dirs.is_empty();
                if ui.add_enabled(ready, egui::Button::new("Continue")).clicked() {
                    submit = true;
                }
            });

        if submit {
            if let Some(root) = dialog.root.clone() {
                let relevant_directories = dialog.relevant_dirs.clone();
                let parent_extensions = parse_ext_list(&dialog.parent_extensions);
                let child_extensions = parse_ext_list(&dialog.child_extensions);
                self.send(Command::SaveSettings {
                    root_directory: root,
                    relevant_directories,
                    parent_extensions,
                    child_extensions,
                });
            }
        }
    }

    fn show_project_view(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            self.show_status_bar(ui);

            let Some(project_id) = self.selected_project else {
                ui.label("Select a project, or create a new one.");
                ui.weak("Tip: drag and drop an image onto this window to start a project from it.");
                return;
            };
            let Some(project) = self.projects.iter().find(|p| p.id == project_id).cloned() else {
                return;
            };

            ui.horizontal(|ui| {
                ui.heading(&project.name);
                if ui.button("Rescan").clicked() {
                    self.send(Command::RescanProject { project_id });
                }
                if ui.button("Sync relevant directories").clicked() {
                    self.send(Command::SyncRelevantDirectories);
                }
            });
            ui.weak(format!("Folder: {}", project.folder_path.display()));

            let files = self.files_by_project.get(&project_id).cloned().unwrap_or_default();
            let pending_rip = files.iter().any(|f| f.source == FileSource::Rip);
            if pending_rip && ui.button("Move all matched RIP files into project").clicked() {
                self.send(Command::MoveAllMatchedIntoProject { project_id });
            }

            ui.separator();
            show_active_dates(ui, &files);
            ui.separator();

            if files.is_empty() {
                ui.label("No files yet.");
                return;
            }

            let (image_groups, unassigned_iterations, leftovers) = group_files(&files, &self.settings.extension_rules);

            if image_groups.is_empty() {
                ui.weak("No images in this project yet.");
                ui.add_space(8.0);
            }

            for group in &image_groups {
                let tex = self.thumbnail_texture(&group.image);
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        if let Some(tex) = &tex {
                            ui.add(egui::Image::new(tex).max_size(egui::vec2(128.0, 128.0)));
                        } else {
                            ui.weak("(loading...)");
                        }
                        ui.vertical(|ui| {
                            ui.strong(&group.image.file_name);
                            ui.weak(group.image.modified_at.format("%Y-%m-%d").to_string());
                            if ui.small_button("Open").clicked() {
                                if let Err(e) = open::that(&group.image.abs_path) {
                                    self.status = Some(format!("Couldn't open {}: {e}", group.image.file_name));
                                }
                            }
                        });
                    });

                    if !group.iterations.is_empty() {
                        ui.add_space(4.0);
                        ui.indent((group.image.abs_path.clone(), "iterations"), |ui| {
                            ui.label(format!("RIP iterations ({})", group.iterations.len()));
                            for iteration in &group.iterations {
                                self.show_iteration(ui, iteration);
                            }
                        });
                    }
                });
                ui.add_space(8.0);
            }

            if !unassigned_iterations.is_empty() {
                ui.label(format!("Unmatched RIP iterations ({})", unassigned_iterations.len()));
                egui::ScrollArea::vertical().id_salt("unassigned_iterations").max_height(400.0).show(ui, |ui| {
                    for iteration in &unassigned_iterations {
                        self.show_iteration(ui, iteration);
                    }
                });
                ui.add_space(8.0);
            }

            if !leftovers.is_empty() {
                ui.label(format!("Other files ({})", leftovers.len()));
                egui::ScrollArea::vertical().id_salt("leftovers").show(ui, |ui| {
                    for file in &leftovers {
                        self.show_file_row(ui, file);
                    }
                });
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.drain_thumbnail_results();
        let ctx = ui.ctx().clone();
        self.handle_dropped_files(&ctx);

        if self.setup_dialog.is_some() {
            self.show_setup_dialog(&ctx);
            return;
        }

        self.show_sidebar(ui);
        self.show_new_project_menu(&ctx);
        self.show_scratch_dialog(&ctx);
        self.show_project_view(ui);
    }
}

fn reveal_in_explorer(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
}

fn downloads_dir() -> Option<PathBuf> {
    directories::UserDirs::new().and_then(|u| u.download_dir().map(|p| p.to_path_buf()))
}

fn display_or(path: &Option<PathBuf>, empty: &str) -> String {
    path.as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| empty.to_string())
}

fn show_active_dates(ui: &mut egui::Ui, files: &[ProjectFileView]) {
    let mut dates: BTreeSet<NaiveDate> = BTreeSet::new();
    for f in files {
        dates.insert(f.created_at.date_naive());
        dates.insert(f.modified_at.date_naive());
    }
    if dates.is_empty() {
        return;
    }
    ui.label("Active dates:");
    ui.horizontal_wrapped(|ui| {
        for date in dates {
            egui::Frame::default()
                .fill(ui.visuals().widgets.inactive.bg_fill)
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(date.format("%Y-%m-%d").to_string()).small());
                });
        }
    });
}
