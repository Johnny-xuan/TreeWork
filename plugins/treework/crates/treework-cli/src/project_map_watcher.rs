use crate::project_map_read_model::{ProjectMapInvalidation, ProjectMapStore};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

const DEBOUNCE: Duration = Duration::from_millis(120);

enum WatchMessage {
    Filesystem(notify::Result<Event>),
    Shutdown,
}

pub(crate) struct ProjectMapWatcher {
    sender: Sender<WatchMessage>,
    thread: Option<JoinHandle<()>>,
}

impl ProjectMapWatcher {
    pub(crate) fn spawn(
        store: Arc<ProjectMapStore>,
        invalidations: broadcast::Sender<ProjectMapInvalidation>,
    ) -> Result<Self, String> {
        let root = store.root().to_path_buf();
        let treework = root.join(".TreeWork");
        let (sender, receiver) = mpsc::channel();
        let callback_sender = sender.clone();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = callback_sender.send(WatchMessage::Filesystem(event));
        })
        .map_err(|error| format!("cannot create Project Map watcher: {}", error))?;
        watcher
            .watch(&root, RecursiveMode::NonRecursive)
            .map_err(|error| format!("cannot watch project root {}: {}", root.display(), error))?;
        watcher
            .watch(&treework, RecursiveMode::Recursive)
            .map_err(|error| {
                format!(
                    "cannot watch TreeWork state {}: {}",
                    treework.display(),
                    error
                )
            })?;
        let report = store.refresh();
        if let Some(invalidation) = report.invalidation {
            let _ = invalidations.send(invalidation);
        }
        let mut managed_roots = BTreeSet::new();
        reconcile_managed_roots(
            &mut watcher,
            &mut managed_roots,
            store.managed_watch_roots(),
        );

        let thread = thread::spawn(move || {
            run_watcher(watcher, receiver, store, invalidations, root, managed_roots)
        });
        Ok(Self {
            sender,
            thread: Some(thread),
        })
    }
}

impl Drop for ProjectMapWatcher {
    fn drop(&mut self) {
        let _ = self.sender.send(WatchMessage::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_watcher(
    mut watcher: RecommendedWatcher,
    receiver: Receiver<WatchMessage>,
    store: Arc<ProjectMapStore>,
    invalidations: broadcast::Sender<ProjectMapInvalidation>,
    root: PathBuf,
    mut managed_roots: BTreeSet<PathBuf>,
) {
    let mut deadline: Option<Instant> = None;
    loop {
        let timeout = deadline
            .map(|value| value.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(1));
        match receiver.recv_timeout(timeout) {
            Ok(WatchMessage::Shutdown) => break,
            Ok(WatchMessage::Filesystem(Ok(event))) => {
                if event
                    .paths
                    .iter()
                    .any(|path| relevant_path(&root, &managed_roots, path))
                {
                    deadline = Some(Instant::now() + DEBOUNCE);
                }
            }
            Ok(WatchMessage::Filesystem(Err(_))) => {
                deadline = Some(Instant::now() + DEBOUNCE);
            }
            Err(RecvTimeoutError::Timeout) if deadline.is_some() => {
                let report = store.refresh();
                if let Some(invalidation) = report.invalidation {
                    let _ = invalidations.send(invalidation);
                }
                reconcile_managed_roots(
                    &mut watcher,
                    &mut managed_roots,
                    store.managed_watch_roots(),
                );
                deadline = None;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn reconcile_managed_roots(
    watcher: &mut RecommendedWatcher,
    current: &mut BTreeSet<PathBuf>,
    desired: BTreeSet<PathBuf>,
) {
    for removed in current.difference(&desired) {
        let _ = watcher.unwatch(removed);
    }
    for added in desired.difference(current) {
        if added.is_dir() {
            let _ = watcher.watch(added, RecursiveMode::Recursive);
        }
    }
    *current = desired;
}

fn relevant_path(root: &Path, managed_roots: &BTreeSet<PathBuf>, path: &Path) -> bool {
    if path == root.join(".TreeWork.lock") {
        return true;
    }
    let treework = root.join(".TreeWork");
    if path.starts_with(treework.join("state")) || path == treework.join("events.jsonl") {
        return true;
    }
    if is_inspector_document(path)
        && (path.starts_with(&treework) || managed_roots.iter().any(|base| path.starts_with(base)))
    {
        return true;
    }
    false
}

fn is_inspector_document(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some("task_plan.md" | "progress.md" | "findings.md" | "verification.md")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_map_read_model::test_support::TestFixture;

    #[test]
    fn classifies_state_event_lock_and_narrative_paths() {
        let root = PathBuf::from("/tmp/treework-watcher-fixture");
        let managed = BTreeSet::from([PathBuf::from("/tmp/treework-managed/.TreeWork")]);
        assert!(relevant_path(
            &root,
            &managed,
            &root.join(".TreeWork/state/project.json")
        ));
        assert!(relevant_path(
            &root,
            &managed,
            &root.join(".TreeWork/events.jsonl")
        ));
        assert!(relevant_path(&root, &managed, &root.join(".TreeWork.lock")));
        assert!(relevant_path(
            &root,
            &managed,
            &root.join(".TreeWork/branches/alpha/progress.md")
        ));
        assert!(relevant_path(
            &root,
            &managed,
            &PathBuf::from("/tmp/treework-managed/.TreeWork/branches/alpha/findings.md")
        ));
        assert!(!relevant_path(
            &root,
            &managed,
            &root.join(".TreeWork/tree.yaml")
        ));
        assert!(!relevant_path(&root, &managed, &root.join("src/main.rs")));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn debounces_document_notifications_into_one_narrative_invalidation() {
        let fixture = TestFixture::accepted();
        let store = Arc::new(fixture.store());
        let _ = store.refresh();
        let (sender, mut receiver) = broadcast::channel(8);
        let watcher =
            ProjectMapWatcher::spawn(store.clone(), sender).expect("start Project Map watcher");
        tokio::time::sleep(Duration::from_millis(50)).await;

        fixture.write_feature_progress("watcher-updated reality");
        let invalidation = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
            .await
            .expect("watcher timeout")
            .expect("watcher invalidation");
        assert_eq!(invalidation.changes, vec!["narrative"]);
        assert_eq!(
            store
                .branch_detail("feature")
                .expect("updated branch detail")
                .progress
                .current_reality,
            "watcher-updated reality"
        );
        drop(watcher);
    }
}
