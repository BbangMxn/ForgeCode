//! Event handling for TUI

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::time::Duration;
use tokio::sync::mpsc;

/// TUI Events
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TuiEvent {
    /// Key press
    Key(KeyEvent),

    /// Terminal resize
    Resize(u16, u16),

    /// Tick (for animations/updates)
    Tick,

    /// Quit request
    Quit,
}

/// Event handler that runs in background
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<TuiEvent>,
}

impl EventHandler {
    /// Create new event handler
    pub fn new() -> (Self, mpsc::UnboundedSender<TuiEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { rx }, tx)
    }

    /// Start event loop
    pub fn start(tx: mpsc::UnboundedSender<TuiEvent>) {
        tokio::spawn(async move {
            let tick_rate = Duration::from_millis(100);

            loop {
                // Poll for events
                if event::poll(tick_rate).unwrap_or(false) {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            // Windows IME fix: Only process Press events, ignore Release
                            // This prevents double input for Korean/Japanese/Chinese characters
                            if key.kind != KeyEventKind::Press {
                                continue;
                            }
                            
                            // Check for quit
                            if key.code == KeyCode::Char('c')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                let _ = tx.send(TuiEvent::Quit);
                                break;
                            }
                            let _ = tx.send(TuiEvent::Key(key));
                        }
                        Ok(Event::Resize(w, h)) => {
                            let _ = tx.send(TuiEvent::Resize(w, h));
                        }
                        // Handle paste events (some IME use this)
                        Ok(Event::Paste(text)) => {
                            // Convert paste to key events
                            for c in text.chars() {
                                let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
                                let _ = tx.send(TuiEvent::Key(key));
                            }
                        }
                        _ => {}
                    }
                }

                // Send tick
                if tx.send(TuiEvent::Tick).is_err() {
                    break;
                }
            }
        });
    }

    /// Receive next event
    pub async fn next(&mut self) -> Option<TuiEvent> {
        self.rx.recv().await
    }
}
