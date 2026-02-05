//! Diff Renderer - 파일 변경 사항 시각화
//!
//! Claude Code 스타일의 실시간 diff 렌더링
//! - 추가/삭제/수정된 줄 하이라이트
//! - 라인 넘버 표시
//! - 컨텍스트 라인 지원

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::VecDeque;

/// Diff 라인 종류
#[derive(Debug, Clone, PartialEq)]
pub enum DiffLineKind {
    /// 동일한 라인 (컨텍스트)
    Context,
    /// 추가된 라인
    Added,
    /// 삭제된 라인
    Removed,
    /// 수정된 라인 (old)
    ModifiedOld,
    /// 수정된 라인 (new)
    ModifiedNew,
    /// 파일 헤더
    Header,
    /// 청크 헤더 (@@ ... @@)
    ChunkHeader,
}

/// Diff 라인
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_line_num: Option<usize>,
    pub new_line_num: Option<usize>,
}

impl DiffLine {
    pub fn context(content: impl Into<String>, old_num: usize, new_num: usize) -> Self {
        Self {
            kind: DiffLineKind::Context,
            content: content.into(),
            old_line_num: Some(old_num),
            new_line_num: Some(new_num),
        }
    }

    pub fn added(content: impl Into<String>, new_num: usize) -> Self {
        Self {
            kind: DiffLineKind::Added,
            content: content.into(),
            old_line_num: None,
            new_line_num: Some(new_num),
        }
    }

    pub fn removed(content: impl Into<String>, old_num: usize) -> Self {
        Self {
            kind: DiffLineKind::Removed,
            content: content.into(),
            old_line_num: Some(old_num),
            new_line_num: None,
        }
    }

    pub fn header(content: impl Into<String>) -> Self {
        Self {
            kind: DiffLineKind::Header,
            content: content.into(),
            old_line_num: None,
            new_line_num: None,
        }
    }

    pub fn chunk_header(content: impl Into<String>) -> Self {
        Self {
            kind: DiffLineKind::ChunkHeader,
            content: content.into(),
            old_line_num: None,
            new_line_num: None,
        }
    }
}

/// Diff 결과
#[derive(Debug, Clone, Default)]
pub struct DiffResult {
    pub file_path: String,
    pub lines: Vec<DiffLine>,
    pub additions: usize,
    pub deletions: usize,
    pub modifications: usize,
}

impl DiffResult {
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            ..Default::default()
        }
    }

    pub fn push(&mut self, line: DiffLine) {
        match line.kind {
            DiffLineKind::Added => self.additions += 1,
            DiffLineKind::Removed => self.deletions += 1,
            DiffLineKind::ModifiedOld | DiffLineKind::ModifiedNew => {
                self.modifications += 1;
            }
            _ => {}
        }
        self.lines.push(line);
    }

    /// 요약 문자열
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.additions > 0 {
            parts.push(format!("+{}", self.additions));
        }
        if self.deletions > 0 {
            parts.push(format!("-{}", self.deletions));
        }
        if self.modifications > 0 {
            parts.push(format!("~{}", self.modifications / 2)); // old/new 쌍
        }
        if parts.is_empty() {
            "no changes".to_string()
        } else {
            parts.join(" ")
        }
    }
}

/// Diff 생성기
pub struct DiffGenerator {
    /// 컨텍스트 라인 수 (변경 전후)
    context_lines: usize,
}

impl DiffGenerator {
    pub fn new() -> Self {
        Self { context_lines: 3 }
    }

    pub fn with_context(mut self, lines: usize) -> Self {
        self.context_lines = lines;
        self
    }

    /// 두 텍스트 간의 diff 생성
    pub fn diff(&self, old: &str, new: &str, file_path: &str) -> DiffResult {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();

        let mut result = DiffResult::new(file_path);
        
        // 파일 헤더
        result.push(DiffLine::header(format!("--- a/{}", file_path)));
        result.push(DiffLine::header(format!("+++ b/{}", file_path)));

        // LCS 기반 diff 계산
        let lcs = self.compute_lcs(&old_lines, &new_lines);
        let operations = self.backtrack_operations(&old_lines, &new_lines, &lcs);

        // 청크로 그룹화
        let chunks = self.group_into_chunks(&operations, old_lines.len(), new_lines.len());

        for chunk in chunks {
            // 청크 헤더
            result.push(DiffLine::chunk_header(format!(
                "@@ -{},{} +{},{} @@",
                chunk.old_start + 1,
                chunk.old_count,
                chunk.new_start + 1,
                chunk.new_count
            )));

            // 청크 내용
            for op in &chunk.operations {
                match op {
                    DiffOp::Equal(old_idx, new_idx) => {
                        result.push(DiffLine::context(
                            old_lines[*old_idx],
                            *old_idx + 1,
                            *new_idx + 1,
                        ));
                    }
                    DiffOp::Insert(new_idx) => {
                        result.push(DiffLine::added(new_lines[*new_idx], *new_idx + 1));
                    }
                    DiffOp::Delete(old_idx) => {
                        result.push(DiffLine::removed(old_lines[*old_idx], *old_idx + 1));
                    }
                }
            }
        }

        result
    }

    /// LCS (Longest Common Subsequence) 계산
    fn compute_lcs(&self, old: &[&str], new: &[&str]) -> Vec<Vec<usize>> {
        let m = old.len();
        let n = new.len();
        let mut lcs = vec![vec![0; n + 1]; m + 1];

        for i in 1..=m {
            for j in 1..=n {
                if old[i - 1] == new[j - 1] {
                    lcs[i][j] = lcs[i - 1][j - 1] + 1;
                } else {
                    lcs[i][j] = lcs[i - 1][j].max(lcs[i][j - 1]);
                }
            }
        }

        lcs
    }

    /// LCS에서 diff 연산 역추적
    fn backtrack_operations(&self, old: &[&str], new: &[&str], lcs: &[Vec<usize>]) -> Vec<DiffOp> {
        let mut ops = Vec::new();
        let mut i = old.len();
        let mut j = new.len();

        while i > 0 || j > 0 {
            if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
                ops.push(DiffOp::Equal(i - 1, j - 1));
                i -= 1;
                j -= 1;
            } else if j > 0 && (i == 0 || lcs[i][j - 1] >= lcs[i - 1][j]) {
                ops.push(DiffOp::Insert(j - 1));
                j -= 1;
            } else if i > 0 {
                ops.push(DiffOp::Delete(i - 1));
                i -= 1;
            }
        }

        ops.reverse();
        ops
    }

    /// 연속된 변경을 청크로 그룹화
    fn group_into_chunks(
        &self,
        operations: &[DiffOp],
        _old_len: usize,
        _new_len: usize,
    ) -> Vec<DiffChunk> {
        if operations.is_empty() {
            return Vec::new();
        }

        let mut chunks = Vec::new();
        let mut current_chunk: Option<DiffChunk> = None;
        let mut context_buffer: VecDeque<DiffOp> = VecDeque::new();

        for op in operations {
            let is_change = !matches!(op, DiffOp::Equal(_, _));

            if is_change {
                // 변경이 발생하면 새 청크 시작 또는 기존 청크에 추가
                if current_chunk.is_none() {
                    let mut chunk = DiffChunk::default();
                    // 컨텍스트 버퍼의 마지막 N개 추가
                    let context_start = context_buffer.len().saturating_sub(self.context_lines);
                    for (i, ctx_op) in context_buffer.iter().enumerate() {
                        if i >= context_start {
                            if let DiffOp::Equal(old_idx, new_idx) = ctx_op {
                                if chunk.old_start == 0 && chunk.new_start == 0 {
                                    chunk.old_start = *old_idx;
                                    chunk.new_start = *new_idx;
                                }
                                chunk.operations.push(ctx_op.clone());
                            }
                        }
                    }
                    current_chunk = Some(chunk);
                }
                
                if let Some(ref mut chunk) = current_chunk {
                    chunk.operations.push(op.clone());
                }
                context_buffer.clear();
            } else {
                // 컨텍스트 라인
                context_buffer.push_back(op.clone());

                if let Some(ref mut chunk) = current_chunk {
                    // 청크 진행 중이면 컨텍스트 추가
                    chunk.operations.push(op.clone());

                    // 컨텍스트 라인이 충분히 쌓이면 청크 종료
                    if context_buffer.len() > self.context_lines * 2 {
                        // 청크 통계 계산
                        self.finalize_chunk(chunk);
                        chunks.push(current_chunk.take().unwrap());
                    }
                }

                // 버퍼 크기 제한
                while context_buffer.len() > self.context_lines * 2 {
                    context_buffer.pop_front();
                }
            }
        }

        // 마지막 청크 처리
        if let Some(mut chunk) = current_chunk {
            self.finalize_chunk(&mut chunk);
            chunks.push(chunk);
        }

        chunks
    }

    fn finalize_chunk(&self, chunk: &mut DiffChunk) {
        for op in &chunk.operations {
            match op {
                DiffOp::Equal(_, _) => {
                    chunk.old_count += 1;
                    chunk.new_count += 1;
                }
                DiffOp::Delete(_) => {
                    chunk.old_count += 1;
                }
                DiffOp::Insert(_) => {
                    chunk.new_count += 1;
                }
            }
        }
    }
}

impl Default for DiffGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Diff 연산
#[derive(Debug, Clone)]
enum DiffOp {
    Equal(usize, usize), // old_idx, new_idx
    Insert(usize),       // new_idx
    Delete(usize),       // old_idx
}

/// Diff 청크
#[derive(Debug, Clone, Default)]
struct DiffChunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    operations: Vec<DiffOp>,
}

/// Diff 렌더러
pub struct DiffRenderer {
    /// 라인 넘버 표시
    show_line_numbers: bool,
    /// 스타일
    added_style: Style,
    removed_style: Style,
    context_style: Style,
    header_style: Style,
    line_number_style: Style,
}

impl DiffRenderer {
    pub fn new() -> Self {
        Self {
            show_line_numbers: true,
            added_style: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            removed_style: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
            context_style: Style::default().fg(Color::Gray),
            header_style: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            line_number_style: Style::default().fg(Color::DarkGray),
        }
    }

    pub fn with_line_numbers(mut self, show: bool) -> Self {
        self.show_line_numbers = show;
        self
    }

    /// Diff 결과를 ratatui 라인으로 렌더링
    pub fn render(&self, diff: &DiffResult) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // 파일 경로 헤더
        lines.push(Line::from(vec![
            Span::styled("📝 ", Style::default()),
            Span::styled(
                diff.file_path.clone(),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" ({})", diff.summary()),
                Style::default().fg(Color::Gray),
            ),
        ]));

        for diff_line in &diff.lines {
            let rendered = self.render_line(diff_line);
            lines.push(rendered);
        }

        lines
    }

    fn render_line(&self, line: &DiffLine) -> Line<'static> {
        let (prefix, style) = match line.kind {
            DiffLineKind::Context => (" ", self.context_style),
            DiffLineKind::Added => ("+", self.added_style),
            DiffLineKind::Removed => ("-", self.removed_style),
            DiffLineKind::ModifiedOld => ("-", self.removed_style),
            DiffLineKind::ModifiedNew => ("+", self.added_style),
            DiffLineKind::Header => ("", self.header_style),
            DiffLineKind::ChunkHeader => ("", Style::default().fg(Color::Magenta)),
        };

        let mut spans = Vec::new();

        // 라인 넘버
        if self.show_line_numbers && !matches!(line.kind, DiffLineKind::Header | DiffLineKind::ChunkHeader) {
            let old_num = line
                .old_line_num
                .map(|n| format!("{:>4}", n))
                .unwrap_or_else(|| "    ".to_string());
            let new_num = line
                .new_line_num
                .map(|n| format!("{:>4}", n))
                .unwrap_or_else(|| "    ".to_string());

            spans.push(Span::styled(old_num, self.line_number_style));
            spans.push(Span::styled(" ", self.line_number_style));
            spans.push(Span::styled(new_num, self.line_number_style));
            spans.push(Span::styled(" │ ", self.line_number_style));
        }

        spans.push(Span::styled(prefix.to_string(), style));
        spans.push(Span::styled(line.content.clone(), style));

        Line::from(spans)
    }

    /// 인라인 diff (단어 수준)
    pub fn inline_diff(&self, old: &str, new: &str) -> Vec<Span<'static>> {
        let old_words: Vec<&str> = old.split_whitespace().collect();
        let new_words: Vec<&str> = new.split_whitespace().collect();

        let mut spans = Vec::new();
        let mut i = 0;
        let mut j = 0;

        while i < old_words.len() || j < new_words.len() {
            if i < old_words.len() && j < new_words.len() && old_words[i] == new_words[j] {
                spans.push(Span::styled(
                    format!("{} ", old_words[i]),
                    self.context_style,
                ));
                i += 1;
                j += 1;
            } else if j < new_words.len()
                && (i >= old_words.len()
                    || !old_words[i..].contains(&new_words[j]))
            {
                spans.push(Span::styled(
                    format!("{} ", new_words[j]),
                    self.added_style,
                ));
                j += 1;
            } else if i < old_words.len() {
                spans.push(Span::styled(
                    format!("{} ", old_words[i]),
                    self.removed_style.add_modifier(Modifier::CROSSED_OUT),
                ));
                i += 1;
            }
        }

        spans
    }
}

impl Default for DiffRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Streaming Diff - 스트리밍 중 실시간 diff 표시
pub struct StreamingDiff {
    original: String,
    current: String,
    file_path: String,
}

impl StreamingDiff {
    pub fn new(original: impl Into<String>, file_path: impl Into<String>) -> Self {
        Self {
            original: original.into(),
            current: String::new(),
            file_path: file_path.into(),
        }
    }

    /// 스트리밍 텍스트 추가
    pub fn append(&mut self, text: &str) {
        self.current.push_str(text);
    }

    /// 현재 diff 생성
    pub fn diff(&self) -> DiffResult {
        let generator = DiffGenerator::new();
        generator.diff(&self.original, &self.current, &self.file_path)
    }

    /// 완료
    pub fn finish(self) -> String {
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_diff() {
        let old = "line 1\nline 2\nline 3";
        let new = "line 1\nmodified line 2\nline 3";

        let generator = DiffGenerator::new();
        let result = generator.diff(old, new, "test.txt");

        assert!(result.deletions > 0 || result.additions > 0);
    }

    #[test]
    fn test_addition() {
        let old = "line 1\nline 2";
        let new = "line 1\nline 2\nline 3";

        let generator = DiffGenerator::new();
        let result = generator.diff(old, new, "test.txt");

        assert_eq!(result.additions, 1);
        assert_eq!(result.deletions, 0);
    }

    #[test]
    fn test_deletion() {
        let old = "line 1\nline 2\nline 3";
        let new = "line 1\nline 3";

        let generator = DiffGenerator::new();
        let result = generator.diff(old, new, "test.txt");

        assert_eq!(result.deletions, 1);
    }

    #[test]
    fn test_render() {
        let old = "fn main() {}\n";
        let new = "fn main() {\n    println!(\"Hello\");\n}\n";

        let generator = DiffGenerator::new();
        let result = generator.diff(old, new, "main.rs");

        let renderer = DiffRenderer::new();
        let lines = renderer.render(&result);

        assert!(!lines.is_empty());
    }

    #[test]
    fn test_streaming_diff() {
        let original = "Hello World";
        let mut streaming = StreamingDiff::new(original, "test.txt");

        streaming.append("Hello ");
        streaming.append("Rust ");
        streaming.append("World");

        let diff = streaming.diff();
        assert!(!diff.lines.is_empty());
    }
}
