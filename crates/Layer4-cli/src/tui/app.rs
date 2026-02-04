//! Main TUI application
//!
//! ForgeCode TUI의 메인 애플리케이션 루프입니다.
//! - 이벤트 처리 (키보드, 터미널 리사이즈)
//! - Agent 이벤트 스트림 처리
//! - 페이지 렌더링

use crate::tui::components::{SettingsAction, SettingsPage};
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
    let result = loop {
        // Draw UI
        terminal.draw(|frame| {
            let area = frame.area();

            // Render chat page
            app.chat.render(frame, area);

            // Render settings overlay if visible
            if app.settings.is_visible() {
                app.settings.render(frame, area);
            }
        })?;

        // Handle events
        tokio::select! {
            // TUI events
            Some(event) = event_handler.next() => {
                match event {
                    TuiEvent::Quit => break Ok(()),
                    TuiEvent::Key(key) => {
                        // Handle settings page first if visible
                        if app.settings.is_visible() {
                            match app.settings.handle_key(key.code) {
                                SettingsAction::Closed => {}
                                SettingsAction::Saved => {
                                    // TODO: Apply settings
                                }
                                SettingsAction::Error(e) => {
                                    tracing::error!("Settings error: {}", e);
                                }
                                SettingsAction::None => {}
                            }
                            continue;
                        }

                        // Check for settings shortcut (Ctrl+S)
                        if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                            && key.code == crossterm::event::KeyCode::Char('s')
                        {
                            app.settings.show();
                            continue;
                        }

                        // Handle chat page
                        if let Some(action) = app.chat.handle_key(key) {
                            match action {
                                ChatAction::SendMessage(content) => {
                                    agent_rx = Some(app.chat.send_message(content).await);
                                }
                                ChatAction::SlashCommand(cmd) => {
                                    app.chat.handle_slash_command(&cmd);
                                }
                                ChatAction::TogglePause => {
                                    app.chat.toggle_pause().await;
                                }
                                ChatAction::StopAgent => {
                                    app.chat.stop_agent().await;
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
    };

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Main application state
struct App {
    /// Chat page
    chat: ChatPage,
    /// Settings page
    settings: SettingsPage,
}

impl App {
    fn new() -> Self {
        Self {
            chat: ChatPage::new(),
            settings: SettingsPage::new(),
        }
    }
}
