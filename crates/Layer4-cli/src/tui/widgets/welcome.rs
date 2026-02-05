//! Welcome Screen - ForgeCode 시작 화면
//!
//! 첫 실행 시 표시되는 환영 화면입니다.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// ForgeCode ASCII 아트 로고
pub const LOGO: &str = r#"
    ███████╗ ██████╗ ██████╗  ██████╗ ███████╗
    ██╔════╝██╔═══██╗██╔══██╗██╔════╝ ██╔════╝
    █████╗  ██║   ██║██████╔╝██║  ███╗█████╗  
    ██╔══╝  ██║   ██║██╔══██╗██║   ██║██╔══╝  
    ██║     ╚██████╔╝██║  ██║╚██████╔╝███████╗
    ╚═╝      ╚═════╝ ╚═╝  ╚═╝ ╚═════╝ ╚══════╝
           ██████╗ ██████╗ ██████╗ ███████╗
          ██╔════╝██╔═══██╗██╔══██╗██╔════╝
          ██║     ██║   ██║██║  ██║█████╗  
          ██║     ██║   ██║██║  ██║██╔══╝  
          ╚██████╗╚██████╔╝██████╔╝███████╗
           ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝
"#;

/// 간단한 로고 (작은 화면용)
pub const LOGO_SMALL: &str = r#"
  ╔═╗┌─┐┬─┐┌─┐┌─┐  ╔═╗┌─┐┌┬┐┌─┐
  ╠╣ │ │├┬┘│ ┬├┤   ║  │ │ ││├┤ 
  ╚  └─┘┴└─└─┘└─┘  ╚═╝└─┘─┴┘└─┘
"#;

/// 최소 로고 (매우 작은 화면용)
pub const LOGO_MINI: &str = "⚡ ForgeCode";

/// Welcome 화면 위젯
pub struct WelcomeScreen {
    /// 환경 정보 (OS, Shell 등)
    pub os_info: String,
    /// 셸 정보
    pub shell_info: String,
    /// 현재 디렉토리
    pub current_dir: String,
    /// 사용 가능한 도구들
    pub tools: Vec<String>,
    /// 모델 이름
    pub model: String,
    /// 프로바이더 이름
    pub provider: String,
}

impl Default for WelcomeScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl WelcomeScreen {
    pub fn new() -> Self {
        // 환경 감지
        let env = forge_foundation::env_detect::Environment::detect();
        
        let mut tools = Vec::new();
        if env.has_cargo {
            tools.push("cargo".to_string());
        }
        if env.has_node {
            tools.push("node".to_string());
        }
        if env.has_python {
            tools.push("python".to_string());
        }
        if env.has_git {
            tools.push("git".to_string());
        }

        Self {
            os_info: format!("{} ({})", env.os.name(), env.arch),
            shell_info: env.shell.name().to_string(),
            current_dir: env.current_dir.to_string_lossy().to_string(),
            tools,
            model: String::new(),
            provider: String::new(),
        }
    }

    pub fn with_model(mut self, provider: &str, model: &str) -> Self {
        self.provider = provider.to_string();
        self.model = model.to_string();
        self
    }

    /// 로고 선택 (화면 크기에 따라)
    fn select_logo(width: u16) -> &'static str {
        if width >= 60 {
            LOGO
        } else if width >= 30 {
            LOGO_SMALL
        } else {
            LOGO_MINI
        }
    }
}

impl Widget for WelcomeScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme_primary = Color::Cyan;
        let theme_secondary = Color::Yellow;
        let theme_muted = Color::DarkGray;

        // 레이아웃 분할
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(if area.width >= 60 { 14 } else { 5 }), // 로고
                Constraint::Length(3), // 환영 메시지
                Constraint::Min(6),    // 정보 패널
                Constraint::Length(5), // 도움말
            ])
            .split(area);

        // === 로고 ===
        let logo = Self::select_logo(area.width);
        let logo_widget = Paragraph::new(logo)
            .style(Style::default().fg(theme_primary).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        logo_widget.render(chunks[0], buf);

        // === 환영 메시지 ===
        let welcome_text = vec![
            Line::from(vec![
                Span::styled("Welcome to ", Style::default().fg(Color::White)),
                Span::styled("ForgeCode", Style::default().fg(theme_primary).add_modifier(Modifier::BOLD)),
                Span::styled(" - AI Coding Assistant", Style::default().fg(Color::White)),
            ]),
        ];
        let welcome = Paragraph::new(welcome_text).alignment(Alignment::Center);
        welcome.render(chunks[1], buf);

        // === 정보 패널 ===
        let info_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .margin(1)
            .split(chunks[2]);

        // 왼쪽: 환경 정보
        let env_info = vec![
            Line::from(vec![
                Span::styled("  OS: ", Style::default().fg(theme_muted)),
                Span::styled(&self.os_info, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  Shell: ", Style::default().fg(theme_muted)),
                Span::styled(&self.shell_info, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  Dir: ", Style::default().fg(theme_muted)),
                Span::styled(
                    truncate_path(&self.current_dir, info_chunks[0].width.saturating_sub(10) as usize),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Tools: ", Style::default().fg(theme_muted)),
                Span::styled(
                    if self.tools.is_empty() { "None detected".to_string() } else { self.tools.join(", ") },
                    Style::default().fg(Color::Green),
                ),
            ]),
        ];
        
        let env_panel = Paragraph::new(env_info)
            .block(Block::default()
                .title(" 🖥️  Environment ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme_muted)));
        env_panel.render(info_chunks[0], buf);

        // 오른쪽: LLM 정보
        let provider_display = if self.provider.is_empty() { "Not configured" } else { &self.provider };
        let model_display = if self.model.is_empty() { "-" } else { &self.model };
        
        let llm_info = vec![
            Line::from(vec![
                Span::styled("  Provider: ", Style::default().fg(theme_muted)),
                Span::styled(provider_display, Style::default().fg(theme_secondary)),
            ]),
            Line::from(vec![
                Span::styled("  Model: ", Style::default().fg(theme_muted)),
                Span::styled(model_display, Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Status: ", Style::default().fg(theme_muted)),
                Span::styled("● Ready", Style::default().fg(Color::Green)),
            ]),
        ];
        
        let llm_panel = Paragraph::new(llm_info)
            .block(Block::default()
                .title(" 🤖 LLM ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme_muted)));
        llm_panel.render(info_chunks[1], buf);

        // === 도움말 ===
        let help_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Type a message to start chatting", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("?", Style::default().fg(theme_secondary).add_modifier(Modifier::BOLD)),
                Span::styled(" help  ", Style::default().fg(theme_muted)),
                Span::styled("Ctrl+M", Style::default().fg(theme_secondary).add_modifier(Modifier::BOLD)),
                Span::styled(" model  ", Style::default().fg(theme_muted)),
                Span::styled("Ctrl+C", Style::default().fg(theme_secondary).add_modifier(Modifier::BOLD)),
                Span::styled(" quit", Style::default().fg(theme_muted)),
            ]),
        ];
        
        let help = Paragraph::new(help_lines)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme_muted));
        help.render(chunks[3], buf);
    }
}

/// 긴 경로를 잘라서 표시
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    
    // 앞에 ... 추가
    let start = path.len() - max_len + 3;
    format!("...{}", &path[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logo_selection() {
        // 큰 화면: 전체 로고
        let big_logo = WelcomeScreen::select_logo(80);
        assert!(big_logo.len() > 100);

        // 중간 화면: 작은 로고 (LOGO_SMALL은 유니코드로 인해 길이가 다양함)
        let medium_logo = WelcomeScreen::select_logo(40);
        assert!(medium_logo.len() < big_logo.len());

        // 작은 화면: 최소 로고
        let small_logo = WelcomeScreen::select_logo(20);
        assert!(small_logo.len() < 50);
    }

    #[test]
    fn test_truncate_path() {
        assert_eq!(truncate_path("short", 10), "short");
        assert!(truncate_path("/very/long/path/to/file", 15).starts_with("..."));
    }
}
