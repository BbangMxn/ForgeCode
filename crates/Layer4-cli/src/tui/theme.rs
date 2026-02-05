//! Theme System - ForgeCode TUI 테마 및 스타일 정의
//!
//! Claude Code 스타일의 깔끔하고 모던한 디자인

use ratatui::style::{Color, Modifier, Style};

/// ForgeCode 테마
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// 배경색
    pub bg: Color,
    /// 전경색 (기본 텍스트)
    pub fg: Color,
    /// 뮤트된 텍스트 (보조 정보)
    pub muted: Color,
    /// 강조색 (브랜드 컬러)
    pub accent: Color,
    /// 성공
    pub success: Color,
    /// 경고
    pub warning: Color,
    /// 에러
    pub error: Color,
    /// 정보
    pub info: Color,
    /// 보더 색상
    pub border: Color,
    /// 선택된 항목 배경
    pub selection_bg: Color,
    /// 선택된 항목 전경
    pub selection_fg: Color,
    /// 코드 블록 배경
    pub code_bg: Color,
    /// 도구 실행 중 색상
    pub tool_running: Color,
    /// 도구 성공 색상
    pub tool_success: Color,
    /// 도구 실패 색상
    pub tool_error: Color,
}

impl Theme {
    /// 다크 테마 (기본)
    pub fn dark() -> Self {
        Self {
            bg: Color::Rgb(22, 22, 26),           // #16161a - 깊은 다크
            fg: Color::Rgb(220, 220, 224),        // #dcdce0 - 밝은 회색
            muted: Color::Rgb(128, 128, 140),     // #80808c - 뮤트된 회색
            accent: Color::Rgb(120, 180, 255),    // #78b4ff - 부드러운 블루
            success: Color::Rgb(80, 200, 120),    // #50c878 - 에메랄드 그린
            warning: Color::Rgb(255, 200, 80),    // #ffc850 - 골드
            error: Color::Rgb(255, 100, 100),     // #ff6464 - 코랄 레드
            info: Color::Rgb(100, 180, 255),      // #64b4ff - 스카이 블루
            border: Color::Rgb(60, 60, 70),       // #3c3c46 - 미묘한 보더
            selection_bg: Color::Rgb(50, 80, 120),// #325078 - 선택 배경
            selection_fg: Color::Rgb(255, 255, 255),
            code_bg: Color::Rgb(30, 30, 36),      // #1e1e24 - 코드 블록
            tool_running: Color::Rgb(255, 200, 80),
            tool_success: Color::Rgb(80, 200, 120),
            tool_error: Color::Rgb(255, 100, 100),
        }
    }

    /// 라이트 테마
    pub fn light() -> Self {
        Self {
            bg: Color::Rgb(250, 250, 252),
            fg: Color::Rgb(30, 30, 40),
            muted: Color::Rgb(120, 120, 130),
            accent: Color::Rgb(0, 100, 200),
            success: Color::Rgb(30, 150, 80),
            warning: Color::Rgb(200, 150, 0),
            error: Color::Rgb(200, 60, 60),
            info: Color::Rgb(0, 120, 200),
            border: Color::Rgb(220, 220, 225),
            selection_bg: Color::Rgb(200, 220, 250),
            selection_fg: Color::Rgb(0, 0, 0),
            code_bg: Color::Rgb(240, 240, 245),
            tool_running: Color::Rgb(200, 150, 0),
            tool_success: Color::Rgb(30, 150, 80),
            tool_error: Color::Rgb(200, 60, 60),
        }
    }

    /// Monokai 테마
    pub fn monokai() -> Self {
        Self {
            bg: Color::Rgb(39, 40, 34),           // #272822
            fg: Color::Rgb(248, 248, 242),        // #f8f8f2
            muted: Color::Rgb(117, 113, 94),      // #75715e
            accent: Color::Rgb(102, 217, 239),    // #66d9ef - 시안
            success: Color::Rgb(166, 226, 46),    // #a6e22e - 그린
            warning: Color::Rgb(253, 151, 31),    // #fd971f - 오렌지
            error: Color::Rgb(249, 38, 114),      // #f92672 - 핑크
            info: Color::Rgb(102, 217, 239),
            border: Color::Rgb(60, 60, 50),
            selection_bg: Color::Rgb(73, 72, 62),
            selection_fg: Color::Rgb(248, 248, 242),
            code_bg: Color::Rgb(30, 31, 28),
            tool_running: Color::Rgb(253, 151, 31),
            tool_success: Color::Rgb(166, 226, 46),
            tool_error: Color::Rgb(249, 38, 114),
        }
    }

    // === 스타일 헬퍼 메서드 ===

    /// 기본 텍스트 스타일
    pub fn text(&self) -> Style {
        Style::default().fg(self.fg)
    }

    /// 뮤트된 텍스트
    pub fn text_muted(&self) -> Style {
        Style::default().fg(self.muted)
    }

    /// 강조 텍스트
    pub fn text_accent(&self) -> Style {
        Style::default().fg(self.accent)
    }

    /// 볼드 텍스트
    pub fn text_bold(&self) -> Style {
        Style::default().fg(self.fg).add_modifier(Modifier::BOLD)
    }

    /// 헤더 스타일
    pub fn header(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// 보더 스타일
    pub fn border(&self) -> Style {
        Style::default().fg(self.border)
    }

    /// 보더 강조 스타일
    pub fn border_focused(&self) -> Style {
        Style::default().fg(self.accent)
    }

    /// 선택된 항목
    pub fn selected(&self) -> Style {
        Style::default()
            .bg(self.selection_bg)
            .fg(self.selection_fg)
    }

    /// 성공 스타일
    pub fn success(&self) -> Style {
        Style::default().fg(self.success)
    }

    /// 경고 스타일
    pub fn warning(&self) -> Style {
        Style::default().fg(self.warning)
    }

    /// 에러 스타일
    pub fn error(&self) -> Style {
        Style::default().fg(self.error)
    }

    /// 정보 스타일
    pub fn info(&self) -> Style {
        Style::default().fg(self.info)
    }

    /// 코드 블록 스타일
    pub fn code_block(&self) -> Style {
        Style::default().bg(self.code_bg).fg(self.fg)
    }

    /// 유저 메시지 라벨
    pub fn user_label(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// 어시스턴트 메시지 라벨
    pub fn assistant_label(&self) -> Style {
        Style::default()
            .fg(self.success)
            .add_modifier(Modifier::BOLD)
    }

    /// 시스템 메시지 스타일
    pub fn system_message(&self) -> Style {
        Style::default()
            .fg(self.muted)
            .add_modifier(Modifier::ITALIC)
    }

    /// 도구 실행 중
    pub fn tool_running(&self) -> Style {
        Style::default().fg(self.tool_running)
    }

    /// 도구 성공
    pub fn tool_success(&self) -> Style {
        Style::default().fg(self.tool_success)
    }

    /// 도구 에러
    pub fn tool_error(&self) -> Style {
        Style::default().fg(self.tool_error)
    }

    /// 프로그레스 바 스타일
    pub fn progress_bar(&self) -> (Style, Style) {
        (
            Style::default().fg(self.accent),      // 채워진 부분
            Style::default().fg(self.border),      // 빈 부분
        )
    }

    /// 단축키 힌트 스타일
    pub fn keybind(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// 단축키 설명 스타일
    pub fn keybind_desc(&self) -> Style {
        Style::default().fg(self.muted)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// 글로벌 테마 (lazy_static 대신 thread_local 사용)
thread_local! {
    static CURRENT_THEME: std::cell::RefCell<Theme> = std::cell::RefCell::new(Theme::dark());
}

/// 현재 테마 가져오기
pub fn current_theme() -> Theme {
    CURRENT_THEME.with(|t| t.borrow().clone())
}

/// 테마 설정
pub fn set_theme(theme: Theme) {
    CURRENT_THEME.with(|t| *t.borrow_mut() = theme);
}

/// 다크 테마로 설정
pub fn set_dark_theme() {
    set_theme(Theme::dark());
}

/// 라이트 테마로 설정
pub fn set_light_theme() {
    set_theme(Theme::light());
}

/// Monokai 테마로 설정
pub fn set_monokai_theme() {
    set_theme(Theme::monokai());
}

// === 아이콘 상수 ===

pub mod icons {
    /// 폴더 아이콘
    pub const FOLDER: &str = "📁";
    /// 파일 아이콘
    pub const FILE: &str = "📄";
    /// 성공 체크
    pub const CHECK: &str = "✓";
    /// 실패 X
    pub const CROSS: &str = "✗";
    /// 실행 중 스피너
    pub const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
    /// 화살표 오른쪽
    pub const ARROW_RIGHT: &str = "→";
    /// 화살표 아래
    pub const ARROW_DOWN: &str = "↓";
    /// 입력 프롬프트
    pub const PROMPT: &str = "❯";
    /// 생각 중
    pub const THINKING: &str = "💭";
    /// 도구
    pub const TOOL: &str = "🔧";
    /// 코드
    pub const CODE: &str = "📝";
    /// 경고
    pub const WARNING: &str = "⚠";
    /// 에러
    pub const ERROR: &str = "❌";
    /// 정보
    pub const INFO: &str = "ℹ";
    /// 유저
    pub const USER: &str = "👤";
    /// 어시스턴트
    pub const ASSISTANT: &str = "🤖";
    /// 시스템
    pub const SYSTEM: &str = "⚙";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_styles() {
        let theme = Theme::dark();
        
        // 스타일이 올바르게 생성되는지 확인
        let _ = theme.text();
        let _ = theme.header();
        let _ = theme.selected();
        let _ = theme.success();
        let _ = theme.error();
    }

    #[test]
    fn test_theme_switching() {
        set_dark_theme();
        let dark = current_theme();
        
        set_light_theme();
        let light = current_theme();
        
        // 배경색이 다른지 확인
        assert_ne!(format!("{:?}", dark.bg), format!("{:?}", light.bg));
    }
}
