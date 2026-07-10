# Matr Project File Manager - Implementation Plan

## Context

This app manages local project folders for an image-to-print/cut workflow. A project usually starts from an image (most often pulled from Downloads), and PrintFactory RIP later turns that image into `.prt` (job) and `.bmp` (thumbnail) files in its own fixed output directory, which this app can't move or restructure. The goal: make project creation fast, keep each project's files organized under one folder the app owns, and automatically surface the RIP output that belongs to each project without ever silently moving files the user didn't ask to move.

An earlier version of this app modeled things as arbitrary user-linked folders grouped under a project. That model is gone: it required manual per-file assignment whenever folders overlapped, and it didn't match how projects actually get created here. The current model below replaced it.

## Architecture

**Configured directories**, set on first run (and reconfigurable later via the sidebar's "Directories..." button), stored in the single-row `settings` table plus a `relevant_directories` table:
- **Root directory**: app-owned. Every project gets its own folder created directly under this. Exactly one.
- **Relevant directories**: an arbitrary list of folders scanned globally (not owned by any project) and fanned out to whichever project's files match, by filename. Generalizes what used to be a single fixed "RIP directory" (PrintFactory's fixed output location) into any number of such folders — e.g. multiple RIP instances or other external tool output locations. All folders in the list are scanned and matched identically; there's no per-folder scoping.

**Configurable extension relationships** (`model::ExtensionRules`, also in `settings`): which extensions count as a project's source ("parent") files versus derived iteration ("child") output that gets matched to a parent by stripped base name. Defaults to the original hardcoded scheme (`parent = png,jpg,jpeg,bmp`; `child = prt,bmp`) but is fully editable in the setup dialog as two comma-separated lists. An extension in both sets (like the default `bmp`) is ambiguous per-file; see `ExtensionRules::is_child_only` — it's treated as a parent unless a sibling with a child-only extension (like `prt`) shares its stem, in which case it's really iteration output.

**Project creation** (`worker.rs`, `Command::CreateProjectFromImage` / `CreateProjectFromScratch`):
- From an image: pick from Downloads (quick shortcut), browse anywhere, or drag-and-drop onto the window. The app creates `root/<sanitized name>/`, **moves** the seed image into it (this move is automatic and always happens, since the image is what defines the project), and records the image's original filename.
- From scratch: just a name, empty folder, no seed image.
- Folder name collisions are resolved by appending " (2)", " (3)", ... (`paths::unique_project_folder`).

**Iteration-output matching** (`scanner::scan_relevant_directories`, `queries::rematch_rip_files`):
- External tools (e.g. PrintFactory RIP) echo the source file's filename in their output, appending "-0", "-1", etc. when the same source is processed more than once.
- A project is **not** limited to a single "seed" file. `scanner::scan_project_home` computes a stripped base name (`scanner::base_name_of`) for every recognized parent file living in the project's own folder under the configured `ExtensionRules`, storing it on `home_files.base_name`. `projects.seed_filename`/`seed_basename` still exist but only for initial folder/name derivation and adoption self-healing — matching no longer reads them.
- Scanning the relevant directories computes the same stripped base name for each configured child-extension file found there and links it to whichever project has a `home_files` row sharing that base name (`rip_files.matched_project_id`, via `rematch_rip_files`). This means every parent file in a project gets its own family of iterations matched, not just the first/original one. All configured relevant directories are scanned together in one pass before reconciling, so files from an already-scanned directory don't get spuriously marked missing by a later directory's scan.
- **Matched files are never moved automatically.** They stay physically in their relevant directory and just show up in the project's unified file view, tagged as unmoved, with a per-file "Move" button and a "Move all matched" button (`organize::move_rip_file_into_project`). This was a deliberate reversal after initially building it as an automatic move.
- Adding a second parent file to an existing project currently only works by copying it into the project's folder externally and hitting "Rescan" (which also re-runs `rematch_rip_files`) — there's no in-app "add file to project" action yet.

**Two source tables instead of one**: `home_files` (physically inside a project's own folder, one folder per project, no overlap possible) and `rip_files` (physically inside the configured relevant directories, linked to a project by filename match rather than by location — the name predates the directory-list generalization but the concept, "iteration output living outside the project's own folder," still fits). `queries::files_for_project` merges both into `ProjectFileView` rows tagged with `FileSource::Home` or `FileSource::Rip` for display. This split is what makes the matching model work: a project's own folder is scanned normally, while the relevant directories are scanned once, globally, and fanned out to whichever projects match.

**Threading**: `eframe`/`egui` renders on the UI thread only. A single background worker thread (`worker.rs`) owns the `rusqlite::Connection` and handles all filesystem/DB work via a `Command`/`UiEvent` channel pair, waking the UI with `ctx.request_repaint()`. Filesystem watching (`watcher.rs`, `notify-debouncer-full`) is live: the worker watches the root directory and every configured relevant directory, feeding `WatchEvent::RootChanged`/`RelevantChanged` into the same `select!` loop as commands, so directory changes are picked up automatically; "Sync now" / "Rescan" / "Sync relevant directories" remain as manual fallbacks.

**Thumbnails** (`app.rs`): images (source images and RIP `.bmp` output) can be print-resolution and slow to decode at full size just to show a small preview, so thumbnail loading is fully off the UI thread. `spawn_thumbnail_loader` starts a small pool of worker threads (sized to `available_parallelism`, capped at 4); the UI (`App::thumbnail_texture`) checks an in-memory cache, kicks off a background request if missing, and shows a "(loading...)" placeholder until the result comes back over a channel (`drain_thumbnail_results`, called each frame). Each worker resizes to a 160px max dimension (`image`'s `.thumbnail()`) before uploading a GPU texture, and also persists the resized result to an on-disk cache (`db::thumbnail_cache_dir()`, keyed by path + mtime + size) so a given file is only ever decoded at full resolution once, not once per app launch.

**Modules**:
```
src/
  main.rs        - thin binary entry point, wires channels + worker + App
  lib.rs         - re-exports the modules below so they're usable from tests
  app.rs         - eframe UI: setup dialog, new-project flow, project view
  model.rs       - Project, HomeFile, RipFile, Settings, ExtensionRules, ProjectFileView, FileKind
  commands.rs    - Command (UI -> worker) / UiEvent (worker -> UI) / WatchEvent
  db/{mod.rs, migrations.rs, queries.rs}
  scanner.rs     - scan_project_home, scan_relevant_directories, base_name_of()
  organize.rs    - move_rip_file_into_project, collision-safe destination naming
  paths.rs       - canonicalize, sanitize_filename, unique_project_folder
  worker.rs      - background thread, command dispatch
```

**Crates**: `eframe`/`egui`/`egui_extras` (UI), `rusqlite` (bundled + chrono), `rusqlite_migration`, `walkdir`, `crossbeam-channel`, `rfd` (folder/file pickers), `chrono`, `dunce` (path canonicalization), `directories` (`%LOCALAPPDATA%` for the DB, `UserDirs` for the Downloads shortcut), `anyhow`/`thiserror`, `tracing`. `notify`/`notify-debouncer-full`/`image` are still dependencies for later milestones (filesystem watching, thumbnails) but nothing uses them yet.

Note: the `eframe`/`egui` version that resolved (0.35) changed the `App` trait from `update(ctx, frame)` to `ui(&mut self, ui: &mut Ui, frame)`, and merged `SidePanel`/`TopBottomPanel` into a single `Panel::left/right/top/bottom(...)`. Code already reflects this.

## What's built

- First-run setup dialog for the root directory, an editable list of relevant directories (add/remove), and editable parent/child extension lists; reconfigurable later.
- Create project from image (Downloads shortcut, browse, or drag-and-drop) or from scratch.
- Project's own folder scanned recursively; all configured relevant directories scanned together and matched by filename.
- Multiple parent files per project, each grouped with its own family of iterations (not a single "original image"), under whatever parent/child extension relationship is configured (defaults to image -> `.prt`/`.bmp`).
- Unified per-project file view merging both sources, tagged by origin.
- Active-dates chip row (union of created/modified dates across a project's files).
- Manual per-file and per-project "move into project" actions for matched iteration files.
- Filesystem watching (auto-refresh on root/relevant-directory change), plus manual rescan/sync as a fallback.
- Thumbnails for images and iteration-output bitmaps: resized, decoded off the UI thread, and disk-cached.

## Not built yet

- An in-app action to add another parent file to an existing project (currently: copy it into the project folder externally, then hit "Rescan").
- Handling the case where two projects share the same base name (currently: most-recently-scanned project wins).
- Per-folder extension scoping for relevant directories (today every configured folder is scanned/matched identically; there's no way to restrict one folder to only certain extensions).
- Named extension groups with explicit parent -> child mappings between groups (today there's exactly one parent set and one child set, not multiple families with independent rules).
- The project view's scroll area has a known bug; low priority since the UI is planned to move to `iced`, which may make the current egui-specific fix moot.

## Verification

- `cargo test` covers: base-name suffix stripping, files matching by filename into a project's unified view without being moved, an explicit move relocating a file and flipping its source tag, unrelated files staying unmatched, iteration output for a second parent file in the same project getting matched too (not just the first/seed file), settings round-tripping a multi-entry relevant-directories list and custom extension rules, output split across two separately configured relevant directories both getting matched, and a fully custom (non-default) parent/child extension relationship matching only the configured extensions.
- Manually: run the app, complete the directory setup dialog (root + at least one relevant directory), create a project from an image, drop matching `-0`/`-1` suffixed files into a configured relevant directory, hit "Sync relevant directories", confirm they appear tagged as unmoved, then confirm "Move" relocates the file and the relevant directory is otherwise untouched.
