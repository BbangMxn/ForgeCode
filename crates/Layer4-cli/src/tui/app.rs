//! Main TUI application

use crate::tui::event::{EventHandler, TuiEvent};
use crate::tui::pages::{ChatAction, ChatPage};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use forge_foundation::ProviderConfig;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tokio::sync::mpsc;

/// Run the TUI application
pub async fn run(config: &ProviderConfig) -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new();

    // Initialize with config
    if let Err(e) = app.chat.init(config) {
        // Show error but continue
        eprintln!("Initialization warning: {}", e);
    }

    // Create event handler
    let (mut event_handler, event_tx) = EventHandler::new();
    EventHandler::start(event_tx);

    // Channel for agent events
    let mut agent_rx: Option<mpsc::Receiver<forge_agent::AgentEvent>> = None;

    // Main loop
    loop {
        // Draw UI
        terminal.draw(|frame| {
            app.chat.render(frame, frame.area());
        })?;

        // Handle events
        tokio::select! {
            // TUI events
            Some(event) = event_handler.next() => {
                match event {
                    TuiEvent::Quit => break,
                    TuiEvent::Key(key) => {
                        if let Some(action) = app.chat.handle_key(key) {
                            match action {
                                ChatAction::SendMessage(content) => {
                                    agent_rx = Some(app.chat.send_message(content).await);
                                }
                            }
                        }
                    }
                    TuiEvent::Resize(_, _) => {
                        // Terminal will handle resize automatically
                    }
                    TuiEvent::Tick => {
                        // Could update animations here
                    }
                }
            }

            // Agent events
            Some(event) = async {
                if let Some(ref mut rx) = agent_rx {
                    rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                app.chat.handle_agent_event(event);
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Main application state
struct App {
    chat: ChatPage,
}

impl App {
    fn new() -> Self {
        Self {
            chat: ChatPage::new(),
        }
    }
}
