use std::path::{Path, PathBuf};
use std::time::Duration;

use crossbeam_channel::Sender;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};

use crate::commands::WatchEvent;

const DEBOUNCE: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct Watchers {
    root: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
    relevant: Vec<Debouncer<RecommendedWatcher, RecommendedCache>>,
}

impl Watchers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn watch_root(&mut self, path: &Path, tx: Sender<WatchEvent>) {
        self.root = build(path, move || {
            let _ = tx.send(WatchEvent::RootChanged);
        });
    }

    pub fn watch_relevant_directories(&mut self, paths: &[PathBuf], tx: Sender<WatchEvent>) {
        self.relevant = paths
            .iter()
            .filter_map(|path| {
                let tx = tx.clone();
                build(path, move || {
                    let _ = tx.send(WatchEvent::RelevantChanged);
                })
            })
            .collect();
    }
}

fn build(
    path: &Path,
    mut on_change: impl FnMut() + Send + 'static,
) -> Option<Debouncer<RecommendedWatcher, RecommendedCache>> {
    let handler = move |result: DebounceEventResult| {
        if matches!(result, Ok(events) if !events.is_empty()) {
            on_change();
        }
    };
    let mut debouncer = new_debouncer(DEBOUNCE, None, handler).ok()?;
    debouncer.watch(path, RecursiveMode::Recursive).ok()?;
    Some(debouncer)
}
