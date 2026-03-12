use crossterm::event::{self, Event as TermEvent, KeyEvent, MouseEvent};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
pub enum Signal {
    Timer,
    Keyboard(KeyEvent),
    Mouse(MouseEvent),
    #[allow(dead_code)]
    TermResize(u16, u16),
}

#[derive(Debug, Clone, Copy)]
pub struct InputConfig {
    pub tick_interval: Duration,
    pub capture_mouse: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_millis(250),
            capture_mouse: true,
        }
    }
}

pub struct InputLoop {
    pub rx: mpsc::Receiver<Signal>,
    #[allow(dead_code)]
    tx: mpsc::Sender<Signal>,
    #[allow(dead_code)]
    worker: thread::JoinHandle<()>,
}

impl InputLoop {
    pub fn start(cfg: InputConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        let worker = {
            let sender = tx.clone();
            thread::spawn(move || {
                let interval = cfg.tick_interval;
                let mut prev_tick = Instant::now();

                loop {
                    let remaining = interval
                        .checked_sub(prev_tick.elapsed())
                        .unwrap_or(Duration::ZERO);

                    if event::poll(remaining).expect("event poll failed") {
                        let ev = event::read().expect("event read failed");
                        let signal = match ev {
                            TermEvent::Key(k) => Some(Signal::Keyboard(k)),
                            TermEvent::Mouse(m) if cfg.capture_mouse => Some(Signal::Mouse(m)),
                            TermEvent::Resize(w, h) => Some(Signal::TermResize(w, h)),
                            _ => None,
                        };
                        if let Some(sig) = signal
                            && sender.send(sig).is_err()
                        {
                            break;
                        }
                    }

                    if prev_tick.elapsed() >= interval {
                        if sender.send(Signal::Timer).is_err() {
                            break;
                        }
                        prev_tick = Instant::now();
                    }
                }
            })
        };

        Self { rx, tx, worker }
    }
}
