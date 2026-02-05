//! Status Bar Widget - ForgeCode 하단 상태 바
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ ↑↓ scroll │ Ctrl+P pause │ Ctrl+M model │ Ctrl+S settings │ ? │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui::theme::{current_theme, Theme};

/// 상태 바 아이템
#[derive(Debug, Clone)]
pub struct StatusItem {
    /// 키 바인딩
    pub key: String,
    /// 설명
    pub description: String,
    /// 활성화 상태
    pub enabled: bool,
}

impl StatusItem {
    pub fn new(key: impl Into<String>, desc: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: desc.into(),
            enabled: true,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// 상태 바 상태
#[derive(Debug, Clone)]
pub struct StatusBarState {
    /// 왼쪽 아이템들
    pub left_items: Vec<StatusItem>,
    /// 오른쪽 아이템들 (우측 정렬)
    pub right_items: Vec<StatusItem>,
    /// 알림 메시지
    pub notification: Option<(String, NotificationType)>,
    /// 알림 타임아웃
    pub notification_timeout: Option<std::time::Instant>,
}

/// 알림 타입
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
}

impl StatusBarState {
    pub fn new() -> Self {
        Self {
            left_items: Self::default_items(),
            right_items: vec![StatusItem::new("?", "help")],
            notification: None,
            notification_timeout: None,
        }
    }

    fn default_items() -> Vec<StatusItem> {
        vec![
            StatusItem::new("↑↓", "scroll"),
            StatusItem::new("Ctrl+P", "pause"),
            StatusItem::new("Ctrl+M", "model"),
            StatusItem::new("Ctrl+S", "settings"),
        ]
    }

    /// 알림 설정
    pub fn notify(&mut self, message: impl Into<String>, notification_type: NotificationType) {
        self.notification = Some((message.into(), notification_type));
        self.notification_timeout =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
    }

    /// 알림 정보
    pub fn info(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationType::Info);
    }

    /// 알림 성공
    pub fn success(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationType::Success);
    }

    /// 알림 경고
    pub fn warning(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationType::Warning);
    }

    /// 알림 에러
    pub fn error(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationType::Error);
    }

    /// 알림 클리어
    pub fn clear_notification(&mut self) {
        self.notification = None;
        self.notification_timeout = None;
    }

    /// 타임아웃 체크
    pub fn check_timeout(&mut self) {
        if let Some(timeout) = self.notification_timeout {
            if std::time::Instant::now() >= timeout {
                self.clear_notification();
            }
        }
    }

    /// 에이전트 실행 중 모드
    pub fn set_running_mode(&mut self) {
        self.left_items = vec![
            StatusItem::new("Ctrl+P", "pause"),
            StatusItem::new("Ctrl+X", "stop"),
            StatusItem::new("Esc", "cancel"),
        ];
    }

    /// 에이전트 일시정지 모드
    pub fn set_paused_mode(&mut self) {
        self.left_items = vec![
            StatusItem::new("Ctrl+P", "resume"),
            StatusItem::new("Ctrl+X", "stop"),
        ];
    }

    /// 기본 모드
    pub fn set_normal_mode(&mut self) {
        self.left_items = Self::default_items();
    }
}

impl Default for StatusBarState {
    fn default() -> Self {
        Self::new()
    }
}

/// 상태 바 위젯
pub struct StatusBar<'a> {
    state: &'a StatusBarState,
    theme: Theme,
}

impl<'a> StatusBar<'a> {
    pub fn new(state: &'a StatusBarState) -> Self {
        Self {
            state,
            theme: current_theme(),
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    fn render_items(&self, items: &[StatusItem]) -> Vec<Span<'static>> {
        let mut spans = Vec::new();

        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" │ ", self.theme.text_muted()));
            }

            let key_style = if item.enabled {
                self.theme.keybind()
            } else {
                self.theme.text_muted()
            };

            let desc_style = if item.enabled {
                self.theme.keybind_desc()
            } else {
                self.theme.text_muted()
            };

            spans.push(Span::styled(item.key.clone(), key_style));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(item.description.clone(), desc_style));
        }

        spans
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // 배경
        let block = Block::default()
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_style(self.theme.border());

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 10 || inner.height < 1 {
            return;
        }

        // 알림이 있으면 알림 표시
        if let Some((message, notification_type)) = &self.state.notification {
            let style = match notification_type {
                NotificationType::Info => self.theme.info(),
                NotificationType::Success => self.theme.success(),
                NotificationType::Warning => self.theme.warning(),
                NotificationType::Error => self.theme.error(),
            };

            let icon = match notification_type {
                NotificationType::Info => "ℹ",
                NotificationType::Success => "✓",
                NotificationType::Warning => "⚠",
                NotificationType::Error => "✗",
            };

            let notification_line = Line::from(vec![
                Span::raw(" "),
                Span::styled(format!("{} {}", icon, message), style),
            ]);

            Paragraph::new(notification_line)
                .alignment(Alignment::Center)
                .render(inner, buf);
            return;
        }

        // 레이아웃: [왼쪽 아이템들] ... [오른쪽 아이템들]
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(70),
                Constraint::Percentage(30),
            ])
            .split(inner);

        // 왼쪽 아이템들
        let left_spans = self.render_items(&self.state.left_items);
        let left_line = Line::from(vec![Span::raw(" ")].into_iter().chain(left_spans).collect::<Vec<_>>());
        Paragraph::new(left_line)
            .alignment(Alignment::Left)
            .render(chunks[0], buf);

        // 오른쪽 아이템들
        let right_spans = self.render_items(&self.state.right_items);
        let mut right_vec: Vec<Span<'static>> = right_spans;
        right_vec.push(Span::raw(" "));
        let right_line = Line::from(right_vec);
        Paragraph::new(right_line)
            .alignment(Alignment::Right)
            .render(chunks[1], buf);
    }
}

/// 미니 상태 바 (단순 버전)
pub struct MiniStatusBar {
    items: Vec<StatusItem>,
    theme: Theme,
}

impl MiniStatusBar {
    pub fn new(items: Vec<StatusItem>) -> Self {
        Self {
            items,
            theme: current_theme(),
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }
}

impl Widget for MiniStatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut spans = vec![Span::raw(" ")];

        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" │ ", self.theme.text_muted()));
            }
            spans.push(Span::styled(item.key.clone(), self.theme.keybind()));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(item.description.clone(), self.theme.keybind_desc()));
        }

        let line = Line::from(spans);
        Paragraph::new(line)
            .style(Style::default().bg(self.theme.bg))
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bar_state() {
        let mut state = StatusBarState::new();
        assert!(state.notification.is_none());

        state.info("Test message");
        assert!(state.notification.is_some());
    }

    #[test]
    fn test_mode_switching() {
        let mut state = StatusBarState::new();
        let normal_count = state.left_items.len();

        state.set_running_mode();
        assert_ne!(state.left_items.len(), normal_count);

        state.set_normal_mode();
        assert_eq!(state.left_items.len(), normal_count);
    }
}
