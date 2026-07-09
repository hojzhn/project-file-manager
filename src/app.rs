use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use crossbeam_channel::{Receiver, Sender};
use eframe::egui;

use crate::commands::{Command, UiEvent};
use crate::model::{FileSource, Project, ProjectFileView, ProjectId, Settings, IMAGE_EXTENSIONS};

struct Iteration {
    label: String,
    prt: Option<ProjectFileView>,
    bmp: Option<ProjectFileView>,
}

fn group_files<'a>(
    project: &Project,
    files: &'a [ProjectFileView],
) -> (Option<&'a ProjectFileView>, Vec<Iteration>, Vec<&'a ProjectFileView>) {
    let seed = project
        .seed_filename
        .as_deref()
        .and_then(|seed_name| files.iter().find(|f| f.source == FileSource::Home && f.file_name == seed_name));

    let mut groups: BTreeMap<String, Iteration> = BTreeMap::new();
    let mut leftovers = Vec::new();

    for f in files {
        if Some(f) == seed {
            continue;
        }
        match f.ext.as_str() {
            "prt" | "bmp" => {
                let stem = Path::new(&f.file_name)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| f.file_name.clone());
                let entry = groups.entry(stem.clone()).or_insert_with(|| Iteration {
                    label: stem,
                    prt: None,
                    bmp: None,
                });
                if f.ext == "prt" {
                    entry.prt = Some(f.clone());
                } else {
                    entry.bmp = Some(f.clone());
                }
            }
            _ => leftovers.push(f),
        }
    }

    (seed, groups.into_values().collect(), leftovers)
}

struct SetupDialog {
    root: Option<PathBuf>,
    rip: Option<PathBuf>,
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

    status: Option<String>,
}

impl App {
    pub fn new(command_tx: Sender<Command>, ui_event_rx: Receiver<UiEvent>) -> Self {
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
            status: None,
        }
    }

    fn send(&self, cmd: Command) {
        let _ = self.command_tx.send(cmd);
    }

    fn thumbnail_texture(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
        if let Some(tex) = self.thumbnails.get(path) {
            return Some(tex.clone());
        }
        let decoded = image::open(path).ok()?.into_rgba8();
        let size = [decoded.width() as usize, decoded.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
        let tex = ctx.load_texture(path.to_string_lossy(), color_image, egui::TextureOptions::LINEAR);
        self.thumbnails.insert(path.to_path_buf(), tex.clone());
        Some(tex)
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

    fn drain_events(&mut self) {
        while let Ok(event) = self.ui_event_rx.try_recv() {
            match event {
                UiEvent::Settings(settings) => {
                    if settings.is_configured() {
                        self.setup_dialog = None;
                    } else if self.setup_dialog.is_none() {
                        self.setup_dialog = Some(SetupDialog {
                            root: settings.root_directory.clone(),
                            rip: settings.rip_directory.clone(),
                        });
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
            if is_image_path(&path) {
                self.send(Command::CreateProjectFromImage { image_path: path });
            } else {
                self.status = Some(format!("Dropped file isn't a recognized image: {}", path.display()));
            }
        }
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
                        self.setup_dialog = Some(SetupDialog {
                            root: self.settings.root_directory.clone(),
                            rip: self.settings.rip_directory.clone(),
                        });
                    }
                    if ui
                        .button("⟳ Sync now")
                        .on_hover_text(
                            "Manually re-check the root and RIP directories. \
                             Changes are picked up live, so this is only needed \
                             as a fallback.",
                        )
                        .clicked()
                    {
                        self.send(Command::SyncRootDirectory);
                        self.send(Command::RescanRipDirectory);
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
                if ui.button("📥 Select image from Downloads...").clicked() {
                    let mut dialog = rfd::FileDialog::new().add_filter("Image", IMAGE_EXTENSIONS);
                    if let Some(dir) = downloads_dir() {
                        dialog = dialog.set_directory(dir);
                    }
                    if let Some(path) = dialog.pick_file() {
                        self.send(Command::CreateProjectFromImage { image_path: path });
                    }
                    close = true;
                }
                if ui.button("🖼 Browse for image...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("Image", IMAGE_EXTENSIONS).pick_file() {
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
                    "Choose where project folders are created, and where PrintFactory RIP \
                     writes its output (.prt / .bmp).",
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
                ui.horizontal(|ui| {
                    if ui.button("Choose RIP directory...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            dialog.rip = Some(path);
                        }
                    }
                    ui.weak(display_or(&dialog.rip, "Not set"));
                });

                ui.add_space(8.0);
                let ready = dialog.root.is_some() && dialog.rip.is_some();
                if ui.add_enabled(ready, egui::Button::new("Continue")).clicked() {
                    submit = true;
                }
            });

        if submit {
            if let (Some(root), Some(rip)) = (dialog.root.clone(), dialog.rip.clone()) {
                self.send(Command::SetDirectories { root_directory: root, rip_directory: rip });
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
                if ui.button("Sync RIP directory").clicked() {
                    self.send(Command::RescanRipDirectory);
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

            let (seed, iterations, leftovers) = group_files(&project, &files);
            let seed = seed.cloned();

            if let Some(seed) = &seed {
                let tex = self.thumbnail_texture(ui.ctx(), &seed.abs_path);
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        if let Some(tex) = &tex {
                            ui.add(egui::Image::new(tex).max_size(egui::vec2(128.0, 128.0)));
                        }
                        ui.vertical(|ui| {
                            ui.strong(&seed.file_name);
                            ui.weak("original image");
                            ui.weak(seed.modified_at.format("%Y-%m-%d").to_string());
                            if ui.small_button("Open").clicked() {
                                if let Err(e) = open::that(&seed.abs_path) {
                                    self.status = Some(format!("Couldn't open {}: {e}", seed.file_name));
                                }
                            }
                        });
                    });
                });
                ui.add_space(8.0);
            }

            if !iterations.is_empty() {
                ui.label(format!("RIP iterations ({})", iterations.len()));
                egui::ScrollArea::vertical().id_salt("iterations").max_height(400.0).show(ui, |ui| {
                    for iteration in &iterations {
                        let bmp_tex = iteration.bmp.as_ref().and_then(|bmp| self.thumbnail_texture(ui.ctx(), &bmp.abs_path));
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                if let Some(tex) = &bmp_tex {
                                    ui.add(egui::Image::new(tex).max_size(egui::vec2(96.0, 96.0)));
                                } else {
                                    ui.weak("(no thumbnail yet)");
                                }
                                ui.vertical(|ui| {
                                    ui.label(&iteration.label);
                                    if let Some(prt) = &iteration.prt {
                                        self.show_file_row(ui, prt);
                                    }
                                    if let Some(bmp) = &iteration.bmp {
                                        self.show_file_row(ui, bmp);
                                    }
                                });
                            });
                        });
                    }
                });
                ui.add_space(8.0);
            }

            if !leftovers.is_empty() {
                ui.label(format!("Other files ({})", leftovers.len()));
                egui::ScrollArea::vertical().id_salt("leftovers").show(ui, |ui| {
                    for file in leftovers {
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

fn is_image_path(path: &Path) -> bool {
    path.extension()
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
        .unwrap_or(false)
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
