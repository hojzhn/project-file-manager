# Matr Project File Manager - Implementation Plan

## Context

This app manages local project folders for an image-to-print/cut workflow. A project usually starts from an image (most often pulled from Downloads), and PrintFactory RIP later turns that image into `.prt` (job) and `.bmp` (thumbnail) files in its own fixed output directory, which this app can't move or restructure. The goal: make project creation fast, keep each project's files organized under one folder the app owns, and automatically surface the RIP output that belongs to each project without ever silently moving files the user didn't ask to move.

An earlier version of this app modeled things as arbitrary user-linked folders grouped under a project. That model is gone: it required manual per-file assignment whenever folders overlapped, and it didn't match how projects actually get created here. The current model below replaced it.

## Architecture

**Two configured directories**, set on first run (and reconfigurable later via the sidebar's "Directories..." button), stored in the single-row `settings` table:
- **Root directory**: app-owned. Every project gets its own folder created directly under this.
- **RIP directory**: PrintFactory's fixed output location. Never restructured by this app.

**Project creation** (`worker.rs`, `Command::CreateProjectFromImage` / `CreateProjectFromScratch`):
- From an image: pick from Downloads (quick shortcut), browse anywhere, or drag-and-drop onto the window. The app creates `root/<sanitized name>/`, **moves** the seed image into it (this move is automatic and always happens, since the image is what defines the project), and records the image's original filename.
- From scratch: just a name, empty folder, no seed image.
- Folder name collisions are resolved by appending " (2)", " (3)", ... (`paths::unique_project_folder`).

**RIP file matching** (`scanner::scan_rip_directory`, `queries::rematch_rip_files`):
- PrintFactory echoes the original image's filename in its output, appending "-0", "-1", etc. when the same source is processed more than once.
- Every project stores a `seed_basename` (the seed image's filename, lowercased, with extension and any trailing "-N" stripped) at creation time, so renaming the project later doesn't break matching.
- Scanning the RIP directory computes the same stripped base name for each `.prt`/`.bmp` file found there and links it to whichever project shares that base name (`rip_files.matched_project_id`).
- **Matched files are never moved automatically.** They stay physically in the RIP directory and just show up in the project's unified file view, tagged as unmoved, with a per-file "Move" button and a "Move all matched" button (`organize::move_rip_file_into_project`). This was a deliberate reversal after initially building it as an automatic move.

**Two source tables instead of one**: `home_files` (physically inside a project's own folder, one folder per project, no overlap possible) and `rip_files` (physically inside the single shared RIP directory, linked to a project by filename match rather than by location). `queries::files_for_project` merges both into `ProjectFileView` rows tagged with `FileSource::Home` or `FileSource::Rip` for display. This split is what makes the matching model work: a project's own folder is scanned normally, while the RIP directory is scanned once, globally, and fanned out to whichever projects match.

**Threading**: unchanged from the original design. `eframe`/`egui` renders on the UI thread only; a single background worker thread owns the `rusqlite::Connection` and handles all filesystem/DB work via a `Command`/`UiEvent` channel pair, waking the UI with `ctx.request_repaint()`. A `WatchEvent` channel is stubbed into the worker's `select!` loop for future filesystem-watching but nothing produces events into it yet.

**Modules**:
```
src/
  main.rs        - thin binary entry point, wires channels + worker + App
  lib.rs         - re-exports the modules below so they're usable from tests
  app.rs         - eframe UI: setup dialog, new-project flow, project view
  model.rs       - Project, HomeFile, RipFile, Settings, ProjectFileView, FileKind
  commands.rs    - Command (UI -> worker) / UiEvent (worker -> UI) / WatchEvent
  db/{mod.rs, migrations.rs, queries.rs}
  scanner.rs     - scan_project_home, scan_rip_directory, base_name_of()
  organize.rs    - move_rip_file_into_project, collision-safe destination naming
  paths.rs       - canonicalize, sanitize_filename, unique_project_folder
  worker.rs      - background thread, command dispatch
```

**Crates**: `eframe`/`egui`/`egui_extras` (UI), `rusqlite` (bundled + chrono), `rusqlite_migration`, `walkdir`, `crossbeam-channel`, `rfd` (folder/file pickers), `chrono`, `dunce` (path canonicalization), `directories` (`%LOCALAPPDATA%` for the DB, `UserDirs` for the Downloads shortcut), `anyhow`/`thiserror`, `tracing`. `notify`/`notify-debouncer-full`/`image` are still dependencies for later milestones (filesystem watching, thumbnails) but nothing uses them yet.

Note: the `eframe`/`egui` version that resolved (0.35) changed the `App` trait from `update(ctx, frame)` to `ui(&mut self, ui: &mut Ui, frame)`, and merged `SidePanel`/`TopBottomPanel` into a single `Panel::left/right/top/bottom(...)`. Code already reflects this.

## What's built

- First-run setup dialog for root/RIP directories, reconfigurable later.
- Create project from image (Downloads shortcut, browse, or drag-and-drop) or from scratch.
- Project's own folder scanned recursively; RIP directory scanned and matched by filename.
- Unified per-project file view merging both sources, tagged by origin.
- Active-dates chip row (union of created/modified dates across a project's files).
- Manual per-file and per-project "move into project" actions for matched RIP files.
- Manual rescan (project folder) and manual RIP-directory sync.

## Not built yet

- Filesystem watching (auto-refresh on change instead of manual rescan/sync).
- Thumbnails for images.
- Handling the case where two projects share the same seed basename (currently: most-recently-created project wins).

## Verification

- `cargo test` covers: base-name suffix stripping, RIP files matching by filename into a project's unified view without being moved, an explicit move relocating a file and flipping its source tag, and unrelated RIP files staying unmatched.
- Manually: run the app, complete the directory setup dialog, create a project from an image, drop matching `-0`/`-1` suffixed files into the configured RIP directory, hit "Sync RIP directory", confirm they appear tagged as unmoved, then confirm "Move" relocates the file and the RIP directory is otherwise untouched.
