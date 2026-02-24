use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Resize(u16, u16),
    Tick,
    Render,
    Quit,
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
    _task: tokio::task::JoinHandle<()>,
}

impl EventHandler {
    pub fn new(tick_rate_ms: u64, render_rate_ms: u64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(async move {
            let mut reader = EventStream::new();
            let mut tick_interval =
                tokio::time::interval(Duration::from_millis(tick_rate_ms));
            let mut render_interval =
                tokio::time::interval(Duration::from_millis(render_rate_ms));

            loop {
                tokio::select! {
                    _ = tick_interval.tick() => {
                        if tx.send(AppEvent::Tick).is_err() {
                            break;
                        }
                    }
                    _ = render_interval.tick() => {
                        if tx.send(AppEvent::Render).is_err() {
                            break;
                        }
                    }
                    Some(Ok(event)) = reader.next() => {
                        match event {
                            Event::Key(key) => {
                                // Windows fix: only handle Press events to avoid duplicates
                                if key.kind == KeyEventKind::Press {
                                    if tx.send(AppEvent::Key(key)).is_err() {
                                        break;
                                    }
                                }
                            }
                            Event::Resize(w, h) => {
                                if tx.send(AppEvent::Resize(w, h)).is_err() {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        Self { rx, _task: task }
    }

    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }
}
