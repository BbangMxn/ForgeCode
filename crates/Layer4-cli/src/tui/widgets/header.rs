//! Header Widget - ForgeCode 상단 헤더 바
//!
//! ```text
//! ┌─ ForgeCode ────────────────────────────── claude-sonnet-4 ─┐
//! │ 📁 ~/project                           Context: ████░░ 68% │
//! └────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui::theme::{current_theme, icons, Theme};

/// 헤더 상태 정보
#[derive(Debug, Clone, Default)]
pub struct HeaderState {
    /// 현재 모델명
    pub model: String,
    /// 프로바이더명
    pub provider: String,
    /// 현재 작업 디렉토리
    pub cwd: String,
    /// 세션 ID (축약)
    pub session_id: String,
    /// 컨텍스트 사용률 (0.0 - 1.0)
    pub context_usage: f32,
    /// 토큰 사용량 (입력, 출력)
    pub tokens: (u32, u32),
    /// 에이전트 상태
    pub agent_status: AgentStatus,
    /// 현재 턴
    pub current_turn: u32,
}

/// 에이전트 상태
#[derive(Debug, Clone, Default, PartialEq)]
pub enum AgentStatus {
    #[default]
    Ready,
    Thinking,
    ToolRunning(String),
    Paused,
    Error,
}

impl HeaderState {
    pub fn new() -> Self {
        Self {
            model: "claude-sonnet-4".to_string(),
            provider: "anthropic".to_string(),
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "~".to_string()),
            session_id: String::new(),
            context_usage: 0.0,
            tokens: (0, 0),
            agent_status: AgentStatus::Ready,
            current_turn: 0,
        }
    }

    /// CWD를 축약형으로 표시
    pub fn short_cwd(&self, max_len: usize) -> String {
        let cwd = self.cwd.replace('\\', "/");
        
        // Home 디렉토리 축약
        let home = dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string().replace('\\', "/"))
            .unwrap_or_default();
        
        let cwd = if !home.is_empty() && cwd.starts_with(&home) {
            format!("~{}", &cwd[home.len()..])
        } else {
            cwd
        };

        if cwd.len() <= max_len {
            cwd
        } else {
            format!("...{}", &cwd[cwd.len() - max_len + 3..])
        }
    }

    /// 컨텍스트 사용률 퍼센트
    pub fn context_percent(&self) -> u16 {
        ((self.context_usage * 100.0).min(100.0).max(0.0)) as u16
    }

    /// 상태 텍스트
    pub fn status_text(&self) -> &str {
        match &self.agent_status {
            AgentStatus::Ready => "Ready",
            AgentStatus::Thinking => "Thinking...",
            AgentStatus::ToolRunning(tool) => tool,
            AgentStatus::Paused => "Paused",
            AgentStatus::Error => "Error",
        }
    }
}

/// 헤더 위젯
pub struct Header<'a> {
    state: &'a HeaderState,
    theme: Theme,
}

impl<'a> Header<'a> {
    pub fn new(state: &'a HeaderState) -> Self {
        Self {
            state,
            theme: current_theme(),
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    fn render_title(&self) -> Line<'static> {
        let title_style = self.theme.header();
        let model_style = Style::default()
            .fg(self.theme.muted)
            .add_modifier(Modifier::ITALIC);

        Line::from(vec![
            Span::styled(" ForgeCode ", title_style),
            Span::raw("─".repeat(3)),
            Span::raw(" "),
            Span::styled(self.state.model.clone(), model_style),
            Span::raw(" "),
        ])
    }

    fn render_status_indicator(&self) -> Span<'static> {
        let (symbol, style) = match &self.state.agent_status {
            AgentStatus::Ready => (icons::CHECK, self.theme.success()),
            AgentStatus::Thinking => (icons::THINKING, self.theme.info()),
            AgentStatus::ToolRunning(_) => (icons::TOOL, self.theme.tool_running()),
            AgentStatus::Paused => ("⏸", self.theme.warning()),
            AgentStatus::Error => (icons::ERROR, self.theme.error()),
        };
        Span::styled(format!(" {} ", symbol), style)
    }
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 {
            return;
        }

        // 상단 라인: 타이틀 + 모델
        let block = Block::default()
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
            .border_style(self.theme.border())
            .title(self.render_title())
            .title_alignment(Alignment::Left);

        let inner = block.inner(area);
        block.render(area, buf);

        // 내부 레이아웃
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),  // 왼쪽: CWD + 상태
                Constraint::Percentage(50),  // 오른쪽: 컨텍스트 + 토큰
            ])
            .split(inner);

        // === 왼쪽: CWD + 상태 ===
        let left_content = Line::from(vec![
            Span::styled(format!("{} ", icons::FOLDER), self.theme.text_accent()),
            Span::styled(
                self.state.short_cwd(30),
                self.theme.text(),
            ),
            self.render_status_indicator(),
            Span::styled(
                self.state.status_text().to_string(),
                match self.state.agent_status {
                    AgentStatus::Ready => self.theme.text_muted(),
                    AgentStatus::Thinking => self.theme.info(),
                    AgentStatus::ToolRunning(_) => self.theme.tool_running(),
                    AgentStatus::Paused => self.theme.warning(),
                    AgentStatus::Error => self.theme.error(),
                },
            ),
        ]);

        Paragraph::new(left_content)
            .alignment(Alignment::Left)
            .render(chunks[0], buf);

        // === 오른쪽: 컨텍스트 게이지 + 토큰 ===
        let percent = self.state.context_percent();
        let gauge_color = if percent > 90 {
            self.theme.error
        } else if percent > 70 {
            self.theme.warning
        } else {
            self.theme.success
        };

        // 컨텍스트 게이지 (작은 텍스트 기반)
        let filled = (percent as usize * 6 / 100).min(6);
        let empty = 6 - filled;
        let gauge_str = format!(
            "Context: {}{}",
            "█".repeat(filled),
            "░".repeat(empty)
        );

        let right_content = Line::from(vec![
            Span::styled(gauge_str, Style::default().fg(gauge_color)),
            Span::styled(format!(" {}% ", percent), self.theme.text_muted()),
            Span::raw("│ "),
            Span::styled(
                format!("{}↓ {}↑", self.state.tokens.0, self.state.tokens.1),
                self.theme.text_muted(),
            ),
            Span::raw(" "),
        ]);

        Paragraph::new(right_content)
            .alignment(Alignment::Right)
            .render(chunks[1], buf);
    }
}

/// 스피너 애니메이션 상태
pub struct SpinnerState {
    pub frame: usize,
    last_update: std::time::Instant,
}

impl SpinnerState {
    pub fn new() -> Self {
        Self {
            frame: 0,
            last_update: std::time::Instant::now(),
        }
    }

    pub fn tick(&mut self) -> &'static str {
        if self.last_update.elapsed().as_millis() >= 80 {
            self.frame = (self.frame + 1) % icons::SPINNER.len();
            self.last_update = std::time::Instant::now();
        }
        icons::SPINNER[self.frame]
    }
}

impl Default for SpinnerState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_state() {
        let mut state = HeaderState::new();
        state.context_usage = 0.68;
        assert_eq!(state.context_percent(), 68);
    }

    #[test]
    fn test_short_cwd() {
        let mut state = HeaderState::new();
        state.cwd = "/very/long/path/to/some/project/directory".to_string();
        let short = state.short_cwd(20);
        assert!(short.len() <= 20);
    }
}
