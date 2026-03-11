use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Paste(String),
    Resize(u16, u16),
    Tick,
    Render,
    Quit,
    AiSuggestion(String),
    AiError(String),
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
    pub tx: mpsc::UnboundedSender<AppEvent>,
    tick_rate_ms: u64,
    render_rate_ms: u64,
    _task: Option<tokio::task::JoinHandle<()>>,
}

impl EventHandler {
    /// Create the handler and its channels. Does NOT spawn the event-reading task yet.
    /// Call [`start`] after `enable_raw_mode()` to begin reading terminal events.
    pub fn new(tick_rate_ms: u64, render_rate_ms: u64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel::<AppEvent>();
        Self { rx, tx, tick_rate_ms, render_rate_ms, _task: None }
    }

    /// Spawn the background task that reads terminal events via `EventStream`.
    /// Must be called **after** `crossterm::terminal::enable_raw_mode()` has been called,
    /// otherwise `EventStream::new()` will panic with "reader source not set".
    pub fn start(&mut self) {
        let task_tx = self.tx.clone();
        let tick_rate_ms = self.tick_rate_ms;
        let render_rate_ms = self.render_rate_ms;

        let task = tokio::spawn(async move {
            let mut reader = EventStream::new();
            let mut tick_interval =
                tokio::time::interval(Duration::from_millis(tick_rate_ms));
            let mut render_interval =
                tokio::time::interval(Duration::from_millis(render_rate_ms));

            loop {
                tokio::select! {
                    _ = tick_interval.tick() => {
                        if task_tx.send(AppEvent::Tick).is_err() {
                            break;
                        }
                    }
                    _ = render_interval.tick() => {
                        if task_tx.send(AppEvent::Render).is_err() {
                            break;
                        }
                    }
                    Some(Ok(event)) = reader.next() => {
                        match event {
                            Event::Key(key) => {
                                // Windows fix: only handle Press events to avoid duplicates
                                if key.kind == KeyEventKind::Press {
                                    if task_tx.send(AppEvent::Key(key)).is_err() {
                                        break;
                                    }
                                }
                            }
                            Event::Paste(text) => {
                                if task_tx.send(AppEvent::Paste(text)).is_err() {
                                    break;
                                }
                            }
                            Event::Resize(w, h) => {
                                if task_tx.send(AppEvent::Resize(w, h)).is_err() {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        self._task = Some(task);
    }

    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.tx.clone()
    }
}
