//! Native filesystem observation for the platform-neutral filesystem watcher.

#![forbid(unsafe_code)]

use super::{
    ChangeSink, FileWatcherDrain, FileWatcherDrainOutcome, FileWatcherFactory, FileWatcherSession,
    FileWatcherStartError, ObservedChange, WatcherEpoch, WatcherShutdownControl,
};
use crate::change::{FileChangeKind, FilesystemChange, RelativePath};
use notify::event::{AccessKind, AccessMode, ModifyKind};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use thiserror::Error;

const CHANGE_QUEUE_CAPACITY: usize = 1;
const FILE_WATCHER_QUIET_PERIOD: Duration = Duration::from_millis(250);

pub struct NotifyFileWatcher {
    filter: Arc<dyn Fn(&Path) -> bool + Send + Sync>,
}

impl NotifyFileWatcher {
    pub fn new() -> Self {
        Self::with_filter(|_| true)
    }

    /// Select observed relative paths without assigning domain meaning to them.
    pub fn with_filter(filter: impl Fn(&Path) -> bool + Send + Sync + 'static) -> Self {
        Self {
            filter: Arc::new(filter),
        }
    }
}

impl Default for NotifyFileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl FileWatcherFactory for NotifyFileWatcher {
    fn start(
        &self,
        root: &Path,
        epoch: WatcherEpoch,
        sink: Arc<dyn ChangeSink>,
    ) -> Result<Box<dyn FileWatcherSession>, FileWatcherStartError> {
        let (sender, receiver) = mpsc::sync_channel(CHANGE_QUEUE_CAPACITY);
        let pending = Arc::new(Mutex::new(None));
        let callback_pending = pending.clone();
        let callback_root = root.to_path_buf();
        let filter = self.filter.clone();
        let callback_sender = sender.clone();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Ok(event) => {
                    if let Some(change) =
                        change_from_event(callback_root.as_path(), &event, filter.as_ref())
                    {
                        enqueue(&callback_sender, &callback_pending, change);
                    }
                }
                Err(source) => {
                    let error = NotifyFileWatcherError::Callback(source);
                    tracing::warn!(
                        target: "yss_filesystem::watcher::notify",
                        log_domain = "system",
                        log_event = "watcherError",
                        error = %error,
                        "Filesystem watcher reported an error"
                    );
                    enqueue(
                        &callback_sender,
                        &callback_pending,
                        FilesystemChange::rescan_required(),
                    );
                }
            })
            .map_err(NotifyFileWatcherError::Create)
            .map_err(report_start_error)?;
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(NotifyFileWatcherError::Watch)
            .map_err(report_start_error)?;

        let (completion_sender, completion_receiver) = mpsc::sync_channel(1);
        let worker = spawn_worker(receiver, pending, epoch, sink, completion_sender)
            .map_err(|error| report_start_error(NotifyFileWatcherError::Worker(error)))?;
        Ok(Box::new(NotifyFileWatcherSession {
            sender: Some(sender),
            watcher: Some(watcher),
            completion: Some(completion_receiver),
            worker: Some(worker),
        }))
    }
}

fn enqueue(
    sender: &SyncSender<()>,
    pending: &Mutex<Option<FilesystemChange>>,
    change: FilesystemChange,
) {
    merge_change(
        &mut pending.lock().unwrap_or_else(PoisonError::into_inner),
        change,
    );
    match sender.try_send(()) {
        Ok(()) | Err(mpsc::TrySendError::Full(())) => {}
        Err(mpsc::TrySendError::Disconnected(())) => {
            tracing::warn!("Filesystem watcher worker stopped before accepting a change");
        }
    }
}

fn merge_change(pending: &mut Option<FilesystemChange>, change: FilesystemChange) {
    // Keep a bounded hint for a single path. Multiple paths or lost events still require
    // an authoritative rescan, but repeated writes to one file must not invalidate the project.
    let same_path = match (pending.as_ref(), &change) {
        (None, _) => true,
        (Some(FilesystemChange::File(previous)), FilesystemChange::File(next)) => {
            previous.relative_path() == next.relative_path()
        }
        _ => false,
    };
    *pending = Some(if same_path {
        change
    } else {
        FilesystemChange::rescan_required()
    });
}

fn change_from_event(
    root: &Path,
    event: &Event,
    filter: &dyn Fn(&Path) -> bool,
) -> Option<FilesystemChange> {
    if event.need_rescan() {
        return Some(FilesystemChange::rescan_required());
    }
    let kind = file_change_kind(&event.kind)?;
    let mut change = None;
    for path in &event.paths {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            return Some(FilesystemChange::rescan_required());
        }
        let Ok(relative) = RelativePath::try_new(relative) else {
            continue;
        };
        // Transaction staging is private; only publication to the live path is observable.
        if !crate::transaction::is_internal_path(relative.as_path()) && filter(relative.as_path()) {
            merge_change(&mut change, FilesystemChange::file(relative, kind));
        }
    }
    change
}

fn file_change_kind(kind: &EventKind) -> Option<FileChangeKind> {
    match kind {
        EventKind::Create(_) => Some(FileChangeKind::Created),
        EventKind::Modify(ModifyKind::Name(_)) => Some(FileChangeKind::Renamed),
        EventKind::Modify(_) => Some(FileChangeKind::Modified),
        EventKind::Remove(_) => Some(FileChangeKind::Removed),
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => Some(FileChangeKind::Modified),
        EventKind::Access(_) => None,
        EventKind::Other | EventKind::Any => Some(FileChangeKind::Modified),
    }
}

fn spawn_worker(
    receiver: Receiver<()>,
    pending: Arc<Mutex<Option<FilesystemChange>>>,
    epoch: WatcherEpoch,
    sink: Arc<dyn ChangeSink>,
    completion: SyncSender<WorkerTerminal>,
) -> Result<JoinHandle<()>, std::io::Error> {
    thread::Builder::new()
        .name("yss-filesystem-watcher".into())
        .spawn(move || {
            let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                while receiver.recv().is_ok() {
                    while receiver.recv_timeout(FILE_WATCHER_QUIET_PERIOD).is_ok() {}
                    let change = pending
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take();
                    if let Some(change) = change {
                        sink.publish(ObservedChange { epoch, change });
                    }
                }
            }));
            let terminal = match terminal {
                Ok(()) => WorkerTerminal::Drained,
                Err(_) => WorkerTerminal::Panicked,
            };
            let _ = completion.send(terminal);
        })
}

#[derive(Clone, Copy)]
enum WorkerTerminal {
    Drained,
    Panicked,
}

struct NotifyFileWatcherSession {
    sender: Option<SyncSender<()>>,
    watcher: Option<RecommendedWatcher>,
    completion: Option<Receiver<WorkerTerminal>>,
    worker: Option<JoinHandle<()>>,
}

impl FileWatcherSession for NotifyFileWatcherSession {
    fn close_admission(mut self: Box<Self>) -> Box<dyn FileWatcherDrain> {
        self.sender.take();
        self.watcher.take();
        Box::new(NotifyFileWatcherDrain {
            completion: self.completion.take(),
            worker: self.worker.take(),
        })
    }
}

impl Drop for NotifyFileWatcherSession {
    fn drop(&mut self) {
        self.sender.take();
        self.watcher.take();
        if let Some(worker) = self.worker.take() {
            spawn_worker_reaper(worker);
        }
    }
}

struct NotifyFileWatcherDrain {
    completion: Option<Receiver<WorkerTerminal>>,
    worker: Option<JoinHandle<()>>,
}

impl FileWatcherDrain for NotifyFileWatcherDrain {
    fn finish(mut self: Box<Self>, control: WatcherShutdownControl) -> FileWatcherDrainOutcome {
        let terminal = match self.completion.as_ref() {
            Some(completion) => match control.remaining() {
                Some(remaining) => match completion.recv_timeout(remaining) {
                    Ok(terminal) => terminal,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        return FileWatcherDrainOutcome::TimedOut(self);
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => WorkerTerminal::Panicked,
                },
                None => match completion.try_recv() {
                    Ok(terminal) => terminal,
                    Err(mpsc::TryRecvError::Empty) => {
                        return FileWatcherDrainOutcome::TimedOut(self);
                    }
                    Err(mpsc::TryRecvError::Disconnected) => WorkerTerminal::Panicked,
                },
            },
            None => WorkerTerminal::Drained,
        };
        self.completion.take();
        if let Some(worker) = self.worker.take() {
            join_worker(worker);
        }
        match terminal {
            WorkerTerminal::Drained => FileWatcherDrainOutcome::Drained,
            WorkerTerminal::Panicked => FileWatcherDrainOutcome::WorkerPanicked,
        }
    }
}

impl Drop for NotifyFileWatcherDrain {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            spawn_worker_reaper(worker);
        }
    }
}

fn join_worker(worker: JoinHandle<()>) {
    let _ = worker.join();
}

fn spawn_worker_reaper(worker: JoinHandle<()>) {
    let _ = thread::Builder::new()
        .name("yss-filesystem-watcher-reaper".into())
        .spawn(move || join_worker(worker));
}

#[derive(Debug, Error)]
enum NotifyFileWatcherError {
    #[error("failed to create the filesystem watcher")]
    Create(#[source] notify::Error),
    #[error("failed to watch the filesystem root")]
    Watch(#[source] notify::Error),
    #[error("filesystem watcher callback failed")]
    Callback(#[source] notify::Error),
    #[error("failed to spawn the filesystem watcher worker")]
    Worker(#[source] std::io::Error),
}

fn report_start_error(error: NotifyFileWatcherError) -> FileWatcherStartError {
    tracing::warn!(
        target: "yss_filesystem::watcher::notify",
        log_domain = "system",
        log_event = "watcherStartFailed",
        error = %error,
        "Failed to start filesystem watcher"
    );
    FileWatcherStartError::StartFailed
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{EventAttributes, RenameMode};
    use std::path::PathBuf;

    fn observe(root: &Path, event: &Event) -> Option<FilesystemChange> {
        change_from_event(root, event, &|_| true)
    }

    fn path_event(root: &Path, relative: impl AsRef<Path>) -> Event {
        Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![root.join(relative)],
            attrs: EventAttributes::default(),
        }
    }

    #[test]
    fn caller_filter_controls_file_admission_without_suppressing_root_rescans() {
        let root = PathBuf::from("watch-root");
        let filter = |path: &Path| path.extension().is_some_and(|extension| extension == "bin");
        assert!(change_from_event(&root, &path_event(&root, "ignored.txt"), &filter).is_none());
        assert!(change_from_event(&root, &path_event(&root, "accepted.bin"), &filter).is_some());
        let root_event =
            Event::new(EventKind::Remove(notify::event::RemoveKind::Folder)).add_path(root.clone());
        assert!(change_from_event(&root, &root_event, &filter).is_some());
    }

    #[test]
    fn observed_paths_outside_the_root_are_rejected() {
        let root = PathBuf::from("watch-root");
        let outside = path_event(
            root.as_path(),
            Path::new("..").join("other").join("metadata.yssbi"),
        );

        assert!(observe(root.as_path(), &outside).is_none());
    }

    #[test]
    fn read_access_is_ignored_while_write_close_and_rename_are_retained() {
        let root = PathBuf::from("watch-root");
        let access = Event {
            kind: EventKind::Access(AccessKind::Read),
            paths: vec![root.join("metadata.yssbi")],
            attrs: EventAttributes::default(),
        };
        let rename = Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            paths: vec![root.join("metadata.yssbi")],
            attrs: EventAttributes::default(),
        };
        let write_closed = Event {
            kind: EventKind::Access(AccessKind::Close(AccessMode::Write)),
            paths: vec![root.join("metadata.yssbi")],
            attrs: EventAttributes::default(),
        };

        assert!(observe(root.as_path(), &access).is_none());
        assert!(observe(root.as_path(), &write_closed).is_some());
        assert!(observe(root.as_path(), &rename).is_some());
    }

    #[test]
    fn mixed_boundary_event_keeps_a_relevant_in_root_path() {
        let root = PathBuf::from("watch-root");
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            paths: vec![
                root.join("..").join("other").join("metadata.yssbi"),
                root.join("metadata.yssbi"),
            ],
            attrs: EventAttributes::default(),
        };

        assert!(observe(root.as_path(), &event).is_some());
        let rescan = Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan);
        assert_eq!(
            observe(root.as_path(), &rescan),
            Some(FilesystemChange::RescanRequired)
        );
    }

    #[test]
    fn pending_rescan_survives_a_full_queue_while_the_sink_is_busy() {
        struct CaptureEpoch(SyncSender<WatcherEpoch>);
        impl FileWatcherFactory for CaptureEpoch {
            fn start(
                &self,
                _: &Path,
                epoch: WatcherEpoch,
                _: Arc<dyn ChangeSink>,
            ) -> Result<Box<dyn FileWatcherSession>, FileWatcherStartError> {
                self.0.send(epoch).unwrap();
                Err(FileWatcherStartError::StartFailed)
            }
        }
        struct BlockingSink {
            published: SyncSender<ObservedChange>,
            resume: std::sync::Mutex<Receiver<()>>,
        }
        impl ChangeSink for BlockingSink {
            fn publish(&self, change: ObservedChange) {
                let _ = self.published.send(change);
                let _ = self.resume.lock().unwrap().recv();
            }
        }
        let (published, observed) = mpsc::sync_channel(1);
        let (resume, waiting) = mpsc::sync_channel(1);
        let sink = Arc::new(BlockingSink {
            published,
            resume: std::sync::Mutex::new(waiting),
        });
        let (epochs, epoch) = mpsc::sync_channel(1);
        let owner = crate::watcher::WatcherState::new(Arc::new(CaptureEpoch(epochs)));
        assert!(owner.watch("test-directory", sink.clone()).is_err());
        let epoch = epoch.recv().unwrap();
        let (sender, receiver) = mpsc::sync_channel(CHANGE_QUEUE_CAPACITY);
        let (complete, completion) = mpsc::sync_channel(1);
        let pending = Arc::new(Mutex::new(None));
        let worker = spawn_worker(receiver, pending.clone(), epoch, sink, complete).unwrap();
        let file = FilesystemChange::file(
            RelativePath::try_new("docs/Notes.md").unwrap(),
            FileChangeKind::Modified,
        );
        enqueue(&sender, &pending, file.clone());
        let first = observed.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(first.change, file);
        for _ in 0..100 {
            enqueue(&sender, &pending, file.clone());
        }
        enqueue(&sender, &pending, FilesystemChange::rescan_required());
        enqueue(&sender, &pending, file);
        resume.send(()).unwrap();
        let second = observed.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(second.change, FilesystemChange::RescanRequired);
        assert_eq!(second.epoch, first.epoch);
        drop(sender);
        resume.send(()).unwrap();
        assert!(matches!(
            completion.recv_timeout(Duration::from_secs(3)).unwrap(),
            WorkerTerminal::Drained
        ));
        join_worker(worker);
    }

    #[test]
    fn atomic_file_save_and_repeated_writes_keep_the_observed_path() {
        let root = Path::new("project");
        let replacement = "docs/.Notes.md.yssbi-replacement-0-507de4fb-7f5f-457b-984d-62ed30c07a5e";
        assert!(observe(root, &path_event(root, replacement)).is_none());
        assert!(observe(root, &path_event(root, ".yssbi-transaction/journal")).is_none());
        assert!(observe(root, &path_event(root, format!("{replacement}.md"))).is_some());
        let rename = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(root.join(replacement))
            .add_path(root.join("docs/Notes.md"));
        let filter = |_: &Path| true;
        let mut pending = change_from_event(root, &rename, &filter);
        assert_eq!(
            pending,
            Some(FilesystemChange::file(
                RelativePath::try_new("docs/Notes.md").unwrap(),
                FileChangeKind::Renamed,
            ))
        );
        let written = Event::new(EventKind::Access(AccessKind::Close(AccessMode::Write)))
            .add_path(root.join("docs/Notes.md"));
        merge_change(
            &mut pending,
            change_from_event(root, &written, &filter).unwrap(),
        );
        assert_eq!(
            pending,
            Some(FilesystemChange::file(
                RelativePath::try_new("docs/Notes.md").unwrap(),
                FileChangeKind::Modified,
            ))
        );
    }

    #[test]
    fn multiple_observed_paths_cannot_be_narrowed_by_a_later_single_path() {
        let root = Path::new("project");
        let rename = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(root.join("docs/Before.md"))
            .add_path(root.join("docs/After.md"));
        let mut pending = observe(root, &rename);
        assert_eq!(pending, Some(FilesystemChange::RescanRequired));
        merge_change(
            &mut pending,
            observe(root, &path_event(root, "docs/After.md")).unwrap(),
        );
        assert_eq!(pending, Some(FilesystemChange::RescanRequired));
    }
}
