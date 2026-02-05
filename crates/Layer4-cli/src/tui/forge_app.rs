//! ForgeCode TUI Application - 메인 앱 통합
//!
//! Claude Code 스타일의 프로페셔널한 TUI 인터페이스
//!
//! ```text
//! ┌─ ForgeCode ────────────────────────────── claude-sonnet-4 ─┐
//! │ 📁 ~/project                           Context: ████░░ 68% │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                             │
//! │  👤 You                                              12:34  │
//! │  Fix the bug in main.rs                                     │
//! │                                                             │
//! │  ─────────────────────────────────────────────────────────  │
//! │                                                             │
//! │  🤖 Assistant                                        12:34  │
//! │  I'll analyze and fix the bug.                              │
//! │                                                             │
//! │  ┌─ read main.rs ──────────────────────── ✓ 0.3s ─┐        │
//! │  │ fn main() {                                    │        │
//! │  │     println!("Hello");                         │        │
//! │  │ }                                              │        │
//! │  └────────────────────────────────────────────────┘        │
//! │                                                             │
//! ├─────────────────────────────────────────────────────────────┤
//! │ ❯ Type your message...                              [INS]  │
//! ├─────────────────────────────────────────────────────────────┤
//! │ ↑↓ scroll │ Ctrl+P pause │ Ctrl+M model │ ? help           │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::Widget,
};

use crate::tui::theme::{current_theme, Theme};
use crate::tui::widgets::{
    AgentStatus, ChatMessage, ChatView, ChatViewState, Header, HeaderState, InputArea,
    InputState, SpinnerState, StatusBar, StatusBarState, ToolBlock,
    ToolExecutionState,
};

/// ForgeCode 앱 상태
pub struct ForgeAppState {
    /// 헤더 상태
    pub header: HeaderState,
    /// 채팅 뷰 상태
    pub chat: ChatViewState,
    /// 입력 상태
    pub input: InputState,
    /// 상태 바 상태
    pub status_bar: StatusBarState,
    /// 스피너 상태
    pub spinner: SpinnerState,
    /// 에이전트 실행 중
    pub agent_running: bool,
    /// 에이전트 일시정지
    pub agent_paused: bool,
}

impl ForgeAppState {
    pub fn new() -> Self {
        Self {
            header: HeaderState::new(),
            chat: ChatViewState::new(),
            input: InputState::new(),
            status_bar: StatusBarState::new(),
            spinner: SpinnerState::new(),
            agent_running: false,
            agent_paused: false,
        }
    }

    // === 에이전트 상태 관리 ===

    /// 에이전트 실행 시작
    pub fn start_agent(&mut self) {
        self.agent_running = true;
        self.agent_paused = false;
        self.header.agent_status = AgentStatus::Thinking;
        self.input.disable("Agent running...");
        self.status_bar.set_running_mode();
    }

    /// 에이전트 일시정지
    pub fn pause_agent(&mut self) {
        self.agent_paused = true;
        self.header.agent_status = AgentStatus::Paused;
        self.status_bar.set_paused_mode();
    }

    /// 에이전트 재개
    pub fn resume_agent(&mut self) {
        self.agent_paused = false;
        self.header.agent_status = AgentStatus::Thinking;
        self.status_bar.set_running_mode();
    }

    /// 에이전트 중지
    pub fn stop_agent(&mut self) {
        self.agent_running = false;
        self.agent_paused = false;
        self.header.agent_status = AgentStatus::Ready;
        self.input.enable();
        self.status_bar.set_normal_mode();
    }

    /// 에이전트 에러
    pub fn agent_error(&mut self, message: &str) {
        self.agent_running = false;
        self.agent_paused = false;
        self.header.agent_status = AgentStatus::Error;
        self.input.enable();
        self.status_bar.set_normal_mode();
        self.status_bar.error(message);
    }

    // === 메시지 관리 ===

    /// 사용자 메시지 추가
    pub fn add_user_message(&mut self, content: String) {
        self.chat.push(ChatMessage::user(content));
    }

    /// 어시스턴트 메시지 시작 (스트리밍)
    pub fn start_assistant_message(&mut self) {
        self.chat.push(ChatMessage::assistant("").streaming());
    }

    /// 어시스턴트 메시지에 텍스트 추가
    pub fn append_text(&mut self, text: &str) {
        self.chat.append_to_last(text);
    }

    /// 어시스턴트 메시지 스트리밍 종료
    pub fn finish_assistant_message(&mut self) {
        if let Some(last) = self.chat.messages.last_mut() {
            last.streaming = false;
        }
    }

    /// 시스템 메시지 추가
    pub fn add_system_message(&mut self, content: String) {
        self.chat.push(ChatMessage::system(content));
    }

    // === 도구 관리 ===

    /// 도구 실행 시작
    pub fn start_tool(&mut self, tool_name: &str) {
        self.header.agent_status = AgentStatus::ToolRunning(tool_name.to_string());
        let block = ToolBlock::new(tool_name);
        self.chat.add_tool_block(block);
    }

    /// 도구 실행 완료
    pub fn finish_tool(&mut self, success: bool, duration_ms: u64, output: &str) {
        let state = if success {
            ToolExecutionState::Success { duration_ms }
        } else {
            ToolExecutionState::Error {
                message: output.to_string(),
            }
        };
        self.chat.update_last_tool(state, Some(output.to_string()));
        self.header.agent_status = AgentStatus::Thinking;
    }

    // === 토큰/컨텍스트 관리 ===

    /// 토큰 사용량 업데이트
    pub fn update_tokens(&mut self, input: u32, output: u32) {
        self.header.tokens.0 += input;
        self.header.tokens.1 += output;
        // 컨텍스트 사용량 추정 (200K 기준)
        self.header.context_usage = (self.header.tokens.0 as f32) / 200_000.0;
    }

    /// 턴 업데이트
    pub fn set_turn(&mut self, turn: u32) {
        self.header.current_turn = turn;
    }

    // === 키보드 이벤트 처리 ===

    /// 키 이벤트 처리
    pub fn handle_key(&mut self, key: KeyEvent) -> ForgeAction {
        // Global shortcuts
        match (key.modifiers, key.code) {
            // Ctrl+C: 종료
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                return ForgeAction::Quit;
            }
            // Ctrl+P: 일시정지/재개
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                if self.agent_running {
                    if self.agent_paused {
                        return ForgeAction::ResumeAgent;
                    } else {
                        return ForgeAction::PauseAgent;
                    }
                }
            }
            // Ctrl+X: 중지
            (KeyModifiers::CONTROL, KeyCode::Char('x')) => {
                if self.agent_running {
                    return ForgeAction::StopAgent;
                }
            }
            // Escape: 중지 또는 입력 취소
            (_, KeyCode::Esc) => {
                if self.agent_running {
                    return ForgeAction::StopAgent;
                } else {
                    self.input.content.clear();
                    self.input.cursor = 0;
                }
            }
            // Ctrl+M: 모델 스위처
            (KeyModifiers::CONTROL, KeyCode::Char('m')) => {
                if !self.agent_running {
                    return ForgeAction::OpenModelSwitcher;
                }
            }
            // Ctrl+S: 설정
            (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                if !self.agent_running {
                    return ForgeAction::OpenSettings;
                }
            }
            // Ctrl+L: 화면 클리어
            (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
                if !self.agent_running {
                    self.chat.clear();
                    self.header.tokens = (0, 0);
                    self.header.context_usage = 0.0;
                    self.header.current_turn = 0;
                }
            }
            // Page Up/Down: 스크롤
            (_, KeyCode::PageUp) => {
                self.chat.scroll_up(10);
            }
            (_, KeyCode::PageDown) => {
                self.chat.scroll_down(10);
            }
            _ => {}
        }

        // 에이전트 실행 중이면 입력 무시
        if self.agent_running {
            return ForgeAction::None;
        }

        // 입력 처리
        match key.code {
            KeyCode::Enter => {
                let content = self.input.take();
                if !content.is_empty() {
                    // Slash command 체크
                    if content.starts_with('/') {
                        return ForgeAction::SlashCommand(content);
                    }
                    return ForgeAction::SendMessage(content);
                }
            }
            KeyCode::Backspace => {
                self.input.backspace();
            }
            KeyCode::Delete => {
                self.input.delete();
            }
            KeyCode::Left => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.input.move_word_left();
                } else {
                    self.input.move_left();
                }
            }
            KeyCode::Right => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.input.move_word_right();
                } else {
                    self.input.move_right();
                }
            }
            KeyCode::Home => {
                self.input.move_home();
            }
            KeyCode::End => {
                self.input.move_end();
            }
            KeyCode::Up => {
                self.input.history_up();
            }
            KeyCode::Down => {
                self.input.history_down();
            }
            KeyCode::Char(c) => {
                self.input.insert(c);
            }
            _ => {}
        }

        ForgeAction::None
    }

    /// 스피너 틱 (애니메이션)
    pub fn tick(&mut self) {
        self.spinner.tick();
        self.status_bar.check_timeout();
    }
}

impl Default for ForgeAppState {
    fn default() -> Self {
        Self::new()
    }
}

/// 앱 액션
#[derive(Debug, Clone, PartialEq)]
pub enum ForgeAction {
    None,
    Quit,
    SendMessage(String),
    SlashCommand(String),
    PauseAgent,
    ResumeAgent,
    StopAgent,
    OpenModelSwitcher,
    OpenSettings,
}

/// ForgeCode 앱 위젯
pub struct ForgeApp<'a> {
    state: &'a ForgeAppState,
    theme: Theme,
}

impl<'a> ForgeApp<'a> {
    pub fn new(state: &'a ForgeAppState) -> Self {
        Self {
            state,
            theme: current_theme(),
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }
}

impl Widget for ForgeApp<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // 전체 레이아웃
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),  // 헤더
                Constraint::Min(10),    // 채팅 영역
                Constraint::Length(3),  // 입력
                Constraint::Length(3),  // 상태 바
            ])
            .split(area);

        // 헤더
        let header = Header::new(&self.state.header).with_theme(self.theme.clone());
        header.render(chunks[0], buf);

        // 채팅 뷰
        let chat = ChatView::new(&self.state.chat)
            .with_theme(self.theme.clone())
            .with_spinner_frame(self.state.spinner.frame);
        chat.render(chunks[1], buf);

        // 입력 영역
        let input = InputArea::new(&self.state.input)
            .with_theme(self.theme.clone())
            .focused(!self.state.agent_running);
        input.render(chunks[2], buf);

        // 상태 바
        let status_bar = StatusBar::new(&self.state.status_bar).with_theme(self.theme.clone());
        status_bar.render(chunks[3], buf);
    }
}

/// 헬프 오버레이
pub struct HelpOverlay {
    theme: Theme,
}

impl HelpOverlay {
    pub fn new() -> Self {
        Self {
            theme: current_theme(),
        }
    }
}

impl Widget for HelpOverlay {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};

        let width = 50.min(area.width.saturating_sub(4));
        let height = 18.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;

        let popup_area = Rect::new(x, y, width, height);

        // Clear background
        Clear.render(popup_area, buf);

        let help_text = r#"
  ForgeCode Keyboard Shortcuts

  Navigation
    ↑/↓        History navigation
    PgUp/PgDn  Scroll messages
    Ctrl+L     Clear screen

  Agent Control
    Enter      Send message
    Ctrl+P     Pause/Resume agent
    Ctrl+X     Stop agent
    Esc        Cancel

  Windows
    Ctrl+M     Model switcher
    Ctrl+S     Settings
    ?          This help

  Press any key to close
"#;

        let block = Block::default()
            .title(" Help ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_focused());

        let paragraph = Paragraph::new(help_text)
            .style(self.theme.text())
            .block(block);

        paragraph.render(popup_area, buf);
    }
}

impl Default for HelpOverlay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forge_app_state() {
        let mut state = ForgeAppState::new();
        assert!(!state.agent_running);

        state.start_agent();
        assert!(state.agent_running);
        assert!(state.input.disabled);

        state.stop_agent();
        assert!(!state.agent_running);
        assert!(!state.input.disabled);
    }

    #[test]
    fn test_message_flow() {
        let mut state = ForgeAppState::new();

        state.add_user_message("Hello".to_string());
        state.start_assistant_message();
        state.append_text("Hi ");
        state.append_text("there!");
        state.finish_assistant_message();

        assert_eq!(state.chat.messages.len(), 2);
        assert_eq!(state.chat.messages[1].content, "Hi there!");
    }

    #[test]
    fn test_key_handling() {
        let mut state = ForgeAppState::new();

        // 일반 입력
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        state.handle_key(key);
        assert_eq!(state.input.content, "a");

        // Enter로 메시지 전송
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key);
        assert!(matches!(action, ForgeAction::SendMessage(_)));
    }
}
