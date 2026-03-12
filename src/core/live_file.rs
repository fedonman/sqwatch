use std::{
    fmt,
    fs::File,
    io::{self, Read, Seek},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use crossbeam::{
    channel::{unbounded, Receiver, RecvError, SendError, Sender},
    select,
};
use notify::{event::ModifyKind, RecursiveMode, Watcher};

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
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let ev = res.unwrap();
                if let notify::EventKind::Modify(ModifyKind::Data(_)) = ev.kind {
                    fs_tx.send(ev.paths).unwrap();
                }
            })
            .unwrap();

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
                                watcher
                                    .unwatch(old)
                                    .unwrap_or_else(|_| panic!("Failed to unwatch {:?}", old));
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
                                        self.output
                                            .send(Err(MonitorError::Watcher(e)))
                                            .unwrap();
                                    }
                                }
                            } else {
                                content_tx.send(Ok(String::new())).unwrap();
                            }
                        }
                    }
                }
                recv(fs_rx) -> _ => { notify_tx.send(()).unwrap(); }
                recv(content_rx) -> msg => {
                    self.output
                        .send(msg.unwrap().map_err(MonitorError::File))
                        .unwrap();
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
            self.channel.send(MonitorMsg::WatchPath(path)).unwrap();
        }
    }
}
