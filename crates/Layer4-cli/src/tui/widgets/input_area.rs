//! Input Area Widget - ForgeCode 입력 영역
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ ❯ Type your message...                              [vim]  │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::Span,
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui::theme::{current_theme, icons, Theme};

/// 입력 모드
#[derive(Debug, Clone, PartialEq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Insert,
    Command,
}

/// 입력 영역 상태
#[derive(Debug, Clone)]
pub struct InputState {
    /// 입력 텍스트
    pub content: String,
    /// 커서 위치
    pub cursor: usize,
    /// 입력 모드
    pub mode: InputMode,
    /// 히스토리
    pub history: Vec<String>,
    /// 히스토리 인덱스
    pub history_index: Option<usize>,
    /// 플레이스홀더
    pub placeholder: String,
    /// 비활성화 상태
    pub disabled: bool,
    /// 비활성화 메시지
    pub disabled_message: Option<String>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor: 0,
            mode: InputMode::Normal,
            history: Vec::new(),
            history_index: None,
            placeholder: "Type your message...".to_string(),
            disabled: false,
            disabled_message: None,
        }
    }

    /// 문자 삽입
    pub fn insert(&mut self, c: char) {
        if self.disabled {
            return;
        }
        self.content.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// 백스페이스
    pub fn backspace(&mut self) {
        if self.disabled || self.cursor == 0 {
            return;
        }
        let prev = self.content[..self.cursor]
            .chars()
            .last()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        self.content.remove(self.cursor - prev);
        self.cursor -= prev;
    }

    /// Delete 키
    pub fn delete(&mut self) {
        if self.disabled || self.cursor >= self.content.len() {
            return;
        }
        self.content.remove(self.cursor);
    }

    /// 커서 왼쪽 이동
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            let prev = self.content[..self.cursor]
                .chars()
                .last()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.cursor -= prev;
        }
    }

    /// 커서 오른쪽 이동
    pub fn move_right(&mut self) {
        if self.cursor < self.content.len() {
            let next = self.content[self.cursor..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.cursor += next;
        }
    }

    /// 줄 시작으로
    pub fn move_home(&mut self) {
        // 멀티라인: 현재 줄의 시작으로
        if let Some(line_start) = self.content[..self.cursor].rfind('\n') {
            self.cursor = line_start + 1;
        } else {
            self.cursor = 0;
        }
    }

    /// 줄 끝으로
    pub fn move_end(&mut self) {
        // 멀티라인: 현재 줄의 끝으로
        if let Some(line_end) = self.content[self.cursor..].find('\n') {
            self.cursor += line_end;
        } else {
            self.cursor = self.content.len();
        }
    }

    /// 새 줄 삽입 (Shift+Enter)
    pub fn insert_newline(&mut self) {
        if self.disabled {
            return;
        }
        self.content.insert(self.cursor, '\n');
        self.cursor += 1;
    }

    /// 현재 줄 삭제 (Ctrl+U)
    pub fn clear_line(&mut self) {
        if self.disabled {
            return;
        }
        // 현재 줄의 시작 찾기
        let line_start = self.content[..self.cursor]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        
        // 커서부터 줄 시작까지 삭제
        self.content.drain(line_start..self.cursor);
        self.cursor = line_start;
    }

    /// 커서부터 줄 끝까지 삭제 (Ctrl+K)
    pub fn kill_to_end(&mut self) {
        if self.disabled {
            return;
        }
        let line_end = self.content[self.cursor..]
            .find('\n')
            .map(|p| self.cursor + p)
            .unwrap_or(self.content.len());
        self.content.drain(self.cursor..line_end);
    }

    /// 전체 내용 삭제
    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor = 0;
    }

    /// 줄 수 계산
    pub fn line_count(&self) -> usize {
        self.content.matches('\n').count() + 1
    }

    /// 현재 줄 번호 (0-indexed)
    pub fn current_line(&self) -> usize {
        self.content[..self.cursor].matches('\n').count()
    }

    /// 단어 단위로 왼쪽 이동
    pub fn move_word_left(&mut self) {
        let before = &self.content[..self.cursor];
        let trimmed = before.trim_end();
        if let Some(pos) = trimmed.rfind(|c: char| c.is_whitespace()) {
            self.cursor = pos + 1;
        } else {
            self.cursor = 0;
        }
    }

    /// 단어 단위로 오른쪽 이동
    pub fn move_word_right(&mut self) {
        let after = &self.content[self.cursor..];
        let trimmed = after.trim_start();
        let skip = after.len() - trimmed.len();
        if let Some(pos) = trimmed.find(|c: char| c.is_whitespace()) {
            self.cursor += skip + pos;
        } else {
            self.cursor = self.content.len();
        }
    }

    /// 내용 가져오고 초기화
    pub fn take(&mut self) -> String {
        let content = std::mem::take(&mut self.content);
        self.cursor = 0;
        self.history_index = None;
        
        // 히스토리에 추가
        if !content.is_empty() {
            self.history.push(content.clone());
        }
        
        content
    }

    /// 이전 히스토리
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        
        let new_index = match self.history_index {
            Some(i) if i > 0 => i - 1,
            Some(_) => return,
            None => self.history.len() - 1,
        };
        
        self.history_index = Some(new_index);
        self.content = self.history[new_index].clone();
        self.cursor = self.content.len();
    }

    /// 다음 히스토리
    pub fn history_down(&mut self) {
        let new_index = match self.history_index {
            Some(i) if i < self.history.len() - 1 => Some(i + 1),
            Some(_) => {
                self.history_index = None;
                self.content.clear();
                self.cursor = 0;
                return;
            }
            None => return,
        };
        
        if let Some(i) = new_index {
            self.history_index = new_index;
            self.content = self.history[i].clone();
            self.cursor = self.content.len();
        }
    }

    /// 비활성화
    pub fn disable(&mut self, message: impl Into<String>) {
        self.disabled = true;
        self.disabled_message = Some(message.into());
    }

    /// 활성화
    pub fn enable(&mut self) {
        self.disabled = false;
        self.disabled_message = None;
    }

    /// 모드 텍스트
    pub fn mode_text(&self) -> &'static str {
        match self.mode {
            InputMode::Normal => "NOR",
            InputMode::Insert => "INS",
            InputMode::Command => "CMD",
        }
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

/// 입력 영역 위젯
pub struct InputArea<'a> {
    state: &'a InputState,
    theme: Theme,
    focused: bool,
}

impl<'a> InputArea<'a> {
    pub fn new(state: &'a InputState) -> Self {
        Self {
            state,
            theme: current_theme(),
            focused: true,
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Widget for InputArea<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.state.disabled {
            self.theme.text_muted()
        } else if self.focused {
            self.theme.border_focused()
        } else {
            self.theme.border()
        };

        let title = if self.state.disabled {
            self.state
                .disabled_message
                .clone()
                .unwrap_or_else(|| " Disabled ".to_string())
        } else {
            " Input ".to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title, border_style));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 10 || inner.height < 1 {
            return;
        }

        // 레이아웃: [프롬프트][내용...][모드 표시]
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(3),       // 프롬프트
                Constraint::Min(10),         // 내용
                Constraint::Length(6),       // 모드 표시
            ])
            .split(inner);

        // 프롬프트
        let prompt_style = if self.state.disabled {
            self.theme.text_muted()
        } else {
            self.theme.text_accent()
        };
        let prompt = Paragraph::new(format!("{} ", icons::PROMPT))
            .style(prompt_style);
        prompt.render(chunks[0], buf);

        // 내용 또는 플레이스홀더
        let (content_text, content_style) = if self.state.content.is_empty() && !self.focused {
            (&self.state.placeholder, self.theme.text_muted())
        } else {
            (&self.state.content, self.theme.text())
        };

        // 커서 위치 계산
        let visible_width = chunks[1].width as usize;
        let cursor_pos = self.state.cursor;
        
        // 스크롤 오프셋 계산 (커서가 보이도록)
        let scroll_offset = if cursor_pos >= visible_width {
            cursor_pos - visible_width + 1
        } else {
            0
        };

        // 보이는 부분만 추출
        let visible_content: String = content_text
            .chars()
            .skip(scroll_offset)
            .take(visible_width)
            .collect();

        let content = Paragraph::new(visible_content).style(content_style);
        content.render(chunks[1], buf);

        // 모드 표시
        let mode_style = match self.state.mode {
            InputMode::Normal => self.theme.text_muted(),
            InputMode::Insert => self.theme.info(),
            InputMode::Command => self.theme.warning(),
        };
        let mode = Paragraph::new(format!("[{}]", self.state.mode_text()))
            .style(mode_style)
            .alignment(Alignment::Right);
        mode.render(chunks[2], buf);

        // 커서 렌더링 (focused이고 disabled가 아닐 때만)
        if self.focused && !self.state.disabled {
            let cursor_x = chunks[1].x + (cursor_pos - scroll_offset).min(visible_width) as u16;
            let cursor_y = chunks[1].y;
            
            if cursor_x < chunks[1].x + chunks[1].width {
                // 커서 위치의 문자 가져오기
                let cursor_char = self.state.content
                    .chars()
                    .nth(cursor_pos)
                    .unwrap_or(' ');
                
                if let Some(cell) = buf.cell_mut((cursor_x, cursor_y)) {
                    cell.set_char(cursor_char)
                        .set_style(
                            Style::default()
                                .bg(self.theme.fg)
                                .fg(self.theme.bg)
                        );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_state() {
        let mut state = InputState::new();
        state.insert('H');
        state.insert('i');
        assert_eq!(state.content, "Hi");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn test_cursor_movement() {
        let mut state = InputState::new();
        state.content = "Hello World".to_string();
        state.cursor = 5;
        
        state.move_left();
        assert_eq!(state.cursor, 4);
        
        state.move_right();
        assert_eq!(state.cursor, 5);
        
        state.move_home();
        assert_eq!(state.cursor, 0);
        
        state.move_end();
        assert_eq!(state.cursor, 11);
    }

    #[test]
    fn test_history() {
        let mut state = InputState::new();
        state.content = "first".to_string();
        state.take();
        state.content = "second".to_string();
        state.take();
        
        assert_eq!(state.history.len(), 2);
        
        state.history_up();
        assert_eq!(state.content, "second");
        
        state.history_up();
        assert_eq!(state.content, "first");
    }
}
