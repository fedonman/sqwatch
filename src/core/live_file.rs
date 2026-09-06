use std::{
    fmt,
    fs::File,
    io::{self, Read, Seek},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use crossbeam::{
    channel::{Receiver, RecvError, SendError, Sender, unbounded},
    select,
};
use notify::{RecursiveMode, Watcher, event::ModifyKind};

type StreamResult = Result<String, MonitorError>;

pub enum MonitorError {
    Watcher(notify::Error),
    File(io::Error),
}

impl fmt::Display for MonitorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MonitorError::Watcher(err) => write!(f, "Watcher error: {}", err),
            MonitorError::File(err) => write!(f, "Read error: {}", err),
        }
    }
}

pub enum MonitorMsg {
    WatchPath(Option<PathBuf>),
}

pub struct LiveFileMonitor {
    channel: Sender<MonitorMsg>,
    tracked_path: Option<PathBuf>,
}

struct FileObserver {
    output: Sender<StreamResult>,
    inbox: Receiver<MonitorMsg>,
    channel_back: Sender<MonitorMsg>,
    watched: Option<PathBuf>,
    poll_interval: Duration,
}

struct IncrementalReader {
    sink: Sender<io::Result<String>>,
    notify_rx: Receiver<()>,
    target: PathBuf,
    poll_interval: Duration,
    buffer: String,
    offset: u64,
}

impl FileObserver {
    fn create(
        output: Sender<StreamResult>,
        inbox: Receiver<MonitorMsg>,
        channel_back: Sender<MonitorMsg>,
        poll_interval: Duration,
    ) -> Self {
        FileObserver {
            output,
            inbox,
            channel_back,
            watched: None,
            poll_interval,
        }
    }

    fn event_loop(&mut self) -> Result<(), RecvError> {
        let (fs_tx, fs_rx) = unbounded();
        let mut watcher =
            match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(ev) = res else { return };
                if let notify::EventKind::Modify(ModifyKind::Data(_)) = ev.kind {
                    let _ = fs_tx.send(ev.paths);
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    // No watcher (e.g. inotify limits on a login node): report it
                    // and end the observer cleanly instead of panicking the thread.
                    let _ = self.output.send(Err(MonitorError::Watcher(e)));
                    return Ok(());
                }
            };

        let (mut content_tx, mut content_rx) = unbounded::<io::Result<String>>();
        let (mut notify_tx, mut notify_rx) = unbounded::<()>();
        let _ = (&content_tx, &notify_rx);

        loop {
            select! {
                recv(self.inbox) -> msg => {
                    match msg? {
                        MonitorMsg::WatchPath(new_path) => {
                            (content_tx, content_rx) = unbounded();
                            (notify_tx, notify_rx) = unbounded::<()>();

                            if let Some(old) = &self.watched {
                                let _ = watcher.unwatch(old);
                                self.watched = None;
                            }

                            if let Some(p) = new_path {
                                match watcher.watch(Path::new(&p), RecursiveMode::NonRecursive) {
                                    Ok(_) => {
                                        self.watched = Some(p.clone());
                                        let interval = self.poll_interval;
                                        let tx = content_tx.clone();
                                        let rx = notify_rx.clone();
                                        thread::spawn(move || {
                                            IncrementalReader::create(tx, rx, p, interval)
                                                .read_loop()
                                        });
                                    }
                                    Err(_) if !p.exists() => {
                                        // File doesn't exist yet (e.g. pending job).
                                        // Poll until it appears, then set up the watch.
                                        let interval = self.poll_interval;
                                        let inbox_tx = self.channel_back.clone();
                                        thread::spawn(move || {
                                            loop {
                                                thread::sleep(interval);
                                                if p.exists() {
                                                    let _ = inbox_tx
                                                        .send(MonitorMsg::WatchPath(Some(p)));
                                                    break;
                                                }
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        let _ = self.output.send(Err(MonitorError::Watcher(e)));
                                    }
                                }
                            } else {
                                let _ = content_tx.send(Ok(String::new()));
                            }
                        }
                    }
                }
                recv(fs_rx) -> _ => { let _ = notify_tx.send(()); }
                recv(content_rx) -> msg => {
                    if let Ok(inner) = msg {
                        let _ = self.output.send(inner.map_err(MonitorError::File));
                    }
                }
            }
        }
    }
}

impl IncrementalReader {
    fn create(
        sink: Sender<io::Result<String>>,
        notify_rx: Receiver<()>,
        target: PathBuf,
        poll_interval: Duration,
    ) -> Self {
        IncrementalReader {
            sink,
            notify_rx,
            target,
            poll_interval,
            buffer: String::new(),
            offset: 0,
        }
    }

    fn read_loop(&mut self) -> Result<(), ()> {
        loop {
            self.read_new_content().map_err(|_| ())?;
            select! {
                recv(self.notify_rx) -> msg => {
                    msg.map_err(|_| ())?;
                }
                default(self.poll_interval) => {}
            }
        }
    }

    fn read_new_content(&mut self) -> Result<(), SendError<io::Result<String>>> {
        let result = File::open(&self.target).and_then(|mut fh| {
            // A truncated or rewritten log is shorter than what has already been
            // read. Seeking past the end of a file is not an error, so without
            // this check the read returns nothing and the pane keeps showing the
            // content from before the truncation.
            if fh.metadata()?.len() < self.offset {
                self.offset = 0;
                self.buffer.clear();
            }
            self.offset = fh.seek(io::SeekFrom::Start(self.offset))?;
            self.offset += fh.read_to_string(&mut self.buffer)? as u64;
            Ok(self.buffer.clone())
        });
        self.sink.send(result)
    }
}

impl LiveFileMonitor {
    pub fn new(output: Sender<StreamResult>, poll_interval: Duration) -> Self {
        let (tx, rx) = unbounded();
        let mut observer = FileObserver::create(output, rx, tx.clone(), poll_interval);
        thread::spawn(move || observer.event_loop());

        Self {
            channel: tx,
            tracked_path: None,
        }
    }

    pub fn set_file_path(&mut self, path: Option<PathBuf>) {
        if self.tracked_path != path {
            self.tracked_path = path.clone();
            let _ = self.channel.send(MonitorMsg::WatchPath(path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env, fs,
        sync::atomic::{AtomicU32, Ordering},
    };

    /// A file in the temp directory that removes itself when the test ends.
    struct TempLog(PathBuf);

    impl TempLog {
        fn with(contents: &str) -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                env::temp_dir().join(format!("sqwatch-tail-{}-{}.log", std::process::id(), n));
            fs::write(&path, contents).unwrap();
            TempLog(path)
        }

        fn rewrite(&self, contents: &str) {
            fs::write(&self.0, contents).unwrap();
        }
    }

    impl Drop for TempLog {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    /// A reader wired up for direct `read_new_content` calls; the notify channel
    /// is only read by `read_loop`, which these tests do not run.
    fn reader_for(path: &Path) -> (IncrementalReader, Receiver<io::Result<String>>) {
        let (sink, out) = unbounded();
        let (_, notify_rx) = unbounded();
        let reader = IncrementalReader::create(
            sink,
            notify_rx,
            path.to_path_buf(),
            Duration::from_millis(10),
        );
        (reader, out)
    }

    #[test]
    fn reads_appended_content_incrementally() {
        let log = TempLog::with("line one\n");
        let (mut reader, out) = reader_for(&log.0);

        reader.read_new_content().unwrap();
        assert_eq!(out.recv().unwrap().unwrap(), "line one\n");

        log.rewrite("line one\nline two\n");
        reader.read_new_content().unwrap();
        assert_eq!(out.recv().unwrap().unwrap(), "line one\nline two\n");
    }

    #[test]
    fn truncated_file_replaces_the_buffer_instead_of_repeating_it() {
        let log = TempLog::with("line one\nline two\nline three\n");
        let (mut reader, out) = reader_for(&log.0);

        reader.read_new_content().unwrap();
        assert_eq!(
            out.recv().unwrap().unwrap(),
            "line one\nline two\nline three\n"
        );

        log.rewrite("RESTARTED\n");
        reader.read_new_content().unwrap();
        assert_eq!(out.recv().unwrap().unwrap(), "RESTARTED\n");
    }

    #[test]
    fn an_emptied_file_empties_the_pane() {
        let log = TempLog::with("some output\n");
        let (mut reader, out) = reader_for(&log.0);

        reader.read_new_content().unwrap();
        assert_eq!(out.recv().unwrap().unwrap(), "some output\n");

        log.rewrite("");
        reader.read_new_content().unwrap();
        assert_eq!(out.recv().unwrap().unwrap(), "");
    }
}
