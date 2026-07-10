use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};
use iced::widget::image::Handle as ImageHandle;

const THUMBNAIL_MAX_DIM: u32 = 160;

pub struct ThumbnailRequest {
    pub path: PathBuf,
    pub modified_unix: i64,
    pub size_bytes: u64,
}

pub fn spawn_loader(
    cache_dir: Option<PathBuf>,
) -> (Sender<ThumbnailRequest>, Receiver<(PathBuf, Option<ImageHandle>)>) {
    let (request_tx, request_rx) = crossbeam_channel::unbounded::<ThumbnailRequest>();
    let (result_tx, result_rx) = crossbeam_channel::unbounded::<(PathBuf, Option<ImageHandle>)>();

    let worker_count = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 4);
    for _ in 0..worker_count {
        let request_rx = request_rx.clone();
        let result_tx = result_tx.clone();
        let cache_dir = cache_dir.clone();
        std::thread::spawn(move || {
            while let Ok(req) = request_rx.recv() {
                let handle = load_thumbnail_rgba(&req, cache_dir.as_deref())
                    .map(|rgba| ImageHandle::from_rgba(rgba.width(), rgba.height(), rgba.into_raw()));
                if result_tx.send((req.path, handle)).is_err() {
                    break;
                }
            }
        });
    }

    (request_tx, result_rx)
}

fn cache_path(cache_dir: &Path, req: &ThumbnailRequest) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    req.path.hash(&mut hasher);
    let path_hash = hasher.finish();
    cache_dir.join(format!("{path_hash:016x}_{}_{}.png", req.modified_unix, req.size_bytes))
}

fn load_thumbnail_rgba(req: &ThumbnailRequest, cache_dir: Option<&Path>) -> Option<image::RgbaImage> {
    let cached_path = cache_dir.map(|dir| cache_path(dir, req));

    if let Some(cached_path) = &cached_path {
        if let Ok(cached) = image::open(cached_path) {
            return Some(cached.into_rgba8());
        }
    }

    let decoded = image::open(&req.path).ok()?.thumbnail(THUMBNAIL_MAX_DIM, THUMBNAIL_MAX_DIM).into_rgba8();

    if let Some(cached_path) = &cached_path {
        if let Some(dir) = cached_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = decoded.save(cached_path);
    }

    Some(decoded)
}
