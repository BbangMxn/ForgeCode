---
name: generate
description: ForgeCode 패턴에 맞는 코드 생성
allowed-tools:
  - Read
  - Write
  - Glob
  - Grep
user-invocable: true
argument-hint:
  - tool <name>
  - skill <name>
  - trait <name>
  - mcp-server <name>
---

# ForgeCode 코드 생성 Skill

ForgeCode의 기존 패턴을 분석하여 일관된 코드를 생성합니다.

## 사용법

```
/generate tool mytool           # 새 Tool 생성
/generate skill myskill         # 새 Skill 생성  
/generate trait MyTrait         # 새 Trait 생성
/generate mcp-server myserver   # MCP 서버 설정 생성
```

## 생성 타입별 상세

### 1. Tool 생성 (`/generate tool <name>`)

위치: `crates/Layer2-core/src/tool/builtin/<name>.rs`

**생성 단계:**
1. 기존 Tool 구현 분석 (read.rs, write.rs, edit.rs 등)
2. Tool trait 패턴 추출
3. 새 Tool 파일 생성
4. builtin/mod.rs에 등록
5. ToolRegistry에 추가

**템플릿:**
```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::tool::{Tool, ToolContext, ToolDefinition, ToolResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct {Name}Input {
    // 입력 필드
}

pub struct {Name}Tool;

impl {Name}Tool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for {Name}Tool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "{name}".into(),
            description: "설명".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(
        &self,
        ctx: &dyn ToolContext,
        input: serde_json::Value,
    ) -> ToolResult {
        let input: {Name}Input = serde_json::from_value(input)?;
        // 구현
        Ok(serde_json::json!({"result": "success"}))
    }
}
```

### 2. Skill 생성 (`/generate skill <name>`)

위치: `crates/Layer2-core/src/skill/builtin/<name>.rs`

**생성 단계:**
1. 기존 Skill 구현 분석 (commit.rs, explain.rs 등)
2. Skill trait 패턴 추출
3. 새 Skill 파일 생성
4. builtin/mod.rs에 등록
5. SkillRegistry에 추가

**템플릿:**
```rust
use async_trait::async_trait;
use crate::skill::{Skill, SkillDefinition, SkillInput, SkillOutput, SkillContext};
use forge_foundation::Result;

pub struct {Name}Skill;

impl {Name}Skill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for {Name}Skill {
    fn definition(&self) -> SkillDefinition {
        SkillDefinition {
            name: "{name}".into(),
            command: "/{name}".into(),
            description: "설명".into(),
            usage: "/{name} [options]".into(),
            arguments: vec![],
            category: "custom".into(),
            user_invocable: true,
        }
    }

    fn system_prompt(&self) -> Option<String> {
        Some(r#"
        시스템 프롬프트...
        "#.into())
    }

    fn requires_agent_loop(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        ctx: &SkillContext<'_>,
        input: SkillInput,
    ) -> Result<SkillOutput> {
        // 구현
        Ok(SkillOutput::success("완료"))
    }
}
```

### 3. Trait 생성 (`/generate trait <Name>`)

**생성 단계:**
1. 계층에 맞는 위치 결정
2. 기존 trait 패턴 분석
3. trait 정의 생성
4. 기본 구현체 생성 (선택)

**템플릿:**
```rust
use async_trait::async_trait;

#[async_trait]
pub trait {Name}: Send + Sync {
    /// 메서드 설명
    async fn method(&self) -> Result<()>;
    
    /// 기본 구현이 있는 메서드
    fn default_method(&self) -> String {
        "default".into()
    }
}
```

### 4. MCP 서버 설정 (`/generate mcp-server <name>`)

위치: `.claude/settings.json` 또는 `.claude/settings.local.json`

**생성 내용:**
```json
{
  "mcpServers": {
    "{name}": {
      "command": "npx",
      "args": ["-y", "@anthropic/{name}-mcp-server"],
      "env": {}
    }
  }
}
```

## 공통 규칙

1. **기존 패턴 분석**: 항상 기존 코드를 먼저 읽고 패턴 추출
2. **네이밍 컨벤션**: 
   - 파일명: snake_case
   - 구조체/트레이트: PascalCase
   - 함수/변수: snake_case
3. **문서화**: 모든 public 항목에 doc comment 추가
4. **에러 처리**: thiserror 사용, 계층별 에러 타입
5. **테스트**: 기본 테스트 케이스 포함

## 파라미터

- `$1`: 생성 타입 (tool, skill, trait, mcp-server)
- `$2`: 이름
- `--dry-run`: 실제 생성 없이 미리보기
- `--with-tests`: 테스트 파일 포함
