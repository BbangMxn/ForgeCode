# AI Agent 연구 조사 (2024-2025)

ForgeCode Agent 시스템 개선을 위한 최신 연구 논문 조사 결과입니다.

## 목차

1. [Agent 아키텍처 Survey](#1-agent-아키텍처-survey)
2. [추론 전략 (Reasoning)](#2-추론-전략-reasoning)
3. [계획 수립 (Planning)](#3-계획-수립-planning)
4. [메모리 시스템 (Memory)](#4-메모리-시스템-memory)
5. [Tool Use & Function Calling](#5-tool-use--function-calling)
6. [Coding Agent](#6-coding-agent)
7. [Multi-Agent 시스템](#7-multi-agent-시스템)
8. [자기 반성 (Self-Reflection)](#8-자기-반성-self-reflection)
9. [구현 우선순위](#9-구현-우선순위)

---

## 1. Agent 아키텍처 Survey

### 핵심 논문

| 논문 | 연도 | 핵심 내용 |
|------|------|----------|
| [Agentic Large Language Models, a survey](https://arxiv.org/html/2503.23037v3) | 2025 | Chain-of-Thought, Self-Reflection 중심의 Agentic LLM 종합 조사 |
| [Large Language Model Agents: A Comprehensive Survey](https://www.preprints.org/manuscript/202512.2119) | 2024 | 100+ 논문 분석, 4가지 카테고리 분류 |
| [From Language to Action: LLMs as Autonomous Agents](https://arxiv.org/html/2508.17281v1) | 2025 | 2023-2025 A*/A 랭크 논문만 분석 |

### Agent 분류 체계 (Taxonomy)

```
LLM Agent
├── Reasoning-Enhanced Agents
│   ├── Chain-of-Thought (CoT)
│   ├── Tree-of-Thought (ToT)
│   └── Graph-of-Thought (GoT)
│
├── Tool-Augmented Agents
│   ├── API 호출
│   ├── 코드 실행
│   └── 웹 브라우징
│
├── Memory-Augmented Agents
│   ├── 단기 메모리 (Working Memory)
│   ├── 장기 메모리 (Episodic Memory)
│   └── RAG 기반 메모리
│
└── Multi-Agent Systems
    ├── 협력 (Cooperation)
    ├── 경쟁 (Competition)
    └── 혼합 (Coopetition)
```

---

## 2. 추론 전략 (Reasoning)

### 2.1 Chain-of-Thought (CoT)

**원리**: 복잡한 문제를 단계별 추론으로 분해

```
User: 15 + 27 = ?

Without CoT: 42

With CoT:
Step 1: 15 = 10 + 5
Step 2: 27 = 20 + 7
Step 3: 10 + 20 = 30
Step 4: 5 + 7 = 12
Step 5: 30 + 12 = 42
Answer: 42
```

**변형들**:
- **Zero-shot CoT**: "Let's think step by step" 프롬프트
- **Few-shot CoT**: 예시와 함께 제공
- **Auto-CoT**: 자동으로 CoT 예시 생성

### 2.2 Tree-of-Thought (ToT)

**원리**: 여러 추론 경로를 탐색하고 최적 선택

```
                    [Problem]
                        │
         ┌──────────────┼──────────────┐
         ▼              ▼              ▼
    [Approach A]   [Approach B]   [Approach C]
    score: 0.7     score: 0.9     score: 0.5
         │              │              │
         ▼              ▼              ▼
    [Expand]       [Expand]       [Prune]
                   score: 0.95
                        │
                        ▼
                   [Solution]
```

**핵심 파라미터**:
- Branching factor: 각 노드에서 생성할 분기 수
- Max depth: 탐색 최대 깊이
- Pruning threshold: 가지치기 임계값

### 2.3 ReAct (Reasoning + Acting)

**원리**: 추론과 행동을 번갈아 수행

```
Thought: I need to find the population of France
Action: search["population of France 2024"]
Observation: France has approximately 68 million people
Thought: Now I have the answer
Action: finish["68 million"]
```

**장점**: Tool 사용과 자연스럽게 통합
**단점**: 긴 작업에서 일관성 유지 어려움

---

## 3. 계획 수립 (Planning)

### 핵심 논문

| 논문 | 연도 | 핵심 내용 |
|------|------|----------|
| [Understanding the Planning of LLM Agents](https://arxiv.org/abs/2402.02716) | 2024 | LLM Agent 계획 수립 종합 조사 |
| [GoalAct: Global Planning and Hierarchical Execution](https://arxiv.org/abs/2504.16563) | 2025 | 전역 계획 + 계층적 실행 |
| [AgentOrchestra: Hierarchical Multi-Agent Framework](https://arxiv.org/html/2506.12508v1) | 2025 | 계층적 다중 Agent 프레임워크 |

### Planning 분류

```
Planning Methods
├── Task Decomposition (작업 분해)
│   ├── Linear: 순차적 하위 작업
│   ├── Hierarchical: 계층적 분해
│   └── Graph-based: DAG 기반 의존성
│
├── Plan Selection (계획 선택)
│   ├── Scoring: 계획별 점수 매기기
│   ├── Voting: 여러 계획 투표
│   └── Search: 탐색 기반 선택
│
├── Plan Revision (계획 수정)
│   ├── Feedback-based: 피드백 기반
│   ├── Self-correction: 자기 수정
│   └── Iterative: 반복적 개선
│
└── External Module (외부 모듈)
    ├── Symbolic Planner: PDDL 등
    ├── RL-based: 강화학습 기반
    └── Hybrid: 혼합 방식
```

### GoalAct 아키텍처 (2025)

```
┌─────────────────────────────────────────┐
│           Global Planner                 │
│  - 전체 작업 분석                        │
│  - High-level 계획 생성                  │
│  - 계획 지속적 업데이트                  │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│        Skill Decomposition               │
│  ┌─────────┬─────────┬─────────┐        │
│  │ Search  │ Coding  │ Writing │        │
│  └─────────┴─────────┴─────────┘        │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│        Hierarchical Executor             │
│  - Skill 단위로 실행                     │
│  - 실패 시 재계획                        │
└─────────────────────────────────────────┘
```

---

## 4. 메모리 시스템 (Memory)

### 핵심 논문

| 논문 | 연도 | 핵심 내용 |
|------|------|----------|
| [A-MEM: Agentic Memory for LLM Agents](https://arxiv.org/abs/2502.12110) | 2025 | Zettelkasten 기반 메모리 네트워크 |
| [Mem0: Production-Ready AI Agents with Long-Term Memory](https://arxiv.org/pdf/2504.19413) | 2025 | 확장 가능한 장기 메모리 |
| [MemGPT](https://arxiv.org/abs/2310.08560) | 2023 | OS 영감 가상 컨텍스트 관리 |

### 메모리 아키텍처 비교

```
┌────────────────────────────────────────────────────────────┐
│                    MemGPT Architecture                      │
├────────────────────────────────────────────────────────────┤
│  Main Context (RAM)     │  External Context (Disk)         │
│  - 현재 대화             │  - 이전 대화 히스토리            │
│  - 활성 정보             │  - 장기 지식                     │
│  - 즉시 접근 가능        │  - 필요시 페이징                 │
└────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│                    A-MEM Architecture                       │
├────────────────────────────────────────────────────────────┤
│  Memory Network (Zettelkasten)                              │
│  ┌─────────┐     ┌─────────┐     ┌─────────┐              │
│  │ Note 1  │────▶│ Note 2  │────▶│ Note 3  │              │
│  │ (fact)  │     │ (code)  │     │(decision)│              │
│  └────┬────┘     └────┬────┘     └────┬────┘              │
│       │              │              │                      │
│       └──────────────┴──────────────┘                      │
│                      │                                      │
│              Dynamic Indexing                               │
└────────────────────────────────────────────────────────────┘
```

### RAG vs Agent Memory

| 특성 | 전통 RAG | Agent Memory |
|------|----------|--------------|
| 소스 | 외부 문서 코퍼스 | 대화 히스토리 |
| Top-k 검색 | 적합 | 중복 문제 |
| 업데이트 | 정적 | 동적 |
| 토큰 효율 | 보통 | A-MEM: 85-93% 절감 |

### 구현 권장사항

```rust
// ForgeCode Memory 계층
pub enum MemoryTier {
    // Tier 1: 즉시 접근 (현재 턴)
    Working {
        messages: Vec<Message>,
        tool_results: Vec<ToolResult>,
    },
    
    // Tier 2: 세션 메모리 (요약된 이전 턴)
    Episodic {
        summaries: Vec<Summary>,
        decisions: Vec<Decision>,
    },
    
    // Tier 3: 장기 메모리 (Zettelkasten)
    Semantic {
        notes: HashMap<NoteId, Note>,
        links: Graph<NoteId>,
    },
}
```

---

## 5. Tool Use & Function Calling

### 핵심 논문

| 논문 | 연도 | 핵심 내용 |
|------|------|----------|
| [Toolformer](https://arxiv.org/abs/2302.04761) | 2023 | Self-supervised tool learning |
| [ToolACE](https://arxiv.org/html/2409.00920v1) | 2024 | 26,507 API 학습, SOTA |
| [Natural Language Tools](https://arxiv.org/html/2510.14453v1) | 2025 | 자연어 기반 tool calling |

### Tool Calling 발전 과정

```
2023: Toolformer
      └─▶ Self-supervised API 학습

2024: Function Calling APIs (OpenAI, Anthropic, Google)
      └─▶ JSON 스키마 기반 구조화된 호출

2024.11: Model Context Protocol (MCP) - Anthropic
      └─▶ 표준화된 tool provider 인터페이스

2025: Natural Language Tools
      └─▶ JSON 제약 없는 자연어 tool 호출
```

### 구조화된 Tool Calling의 문제점

**GSM8K 벤치마크 결과**:
- 자연어 응답: 기준 정확도
- JSON 출력 강제: **-27.3% 정확도 감소**

**원인**:
- 다중 작업 간섭 (Task interference)
- 포맷 제약 + 내용 생성 동시 처리 부담
- 20% 이상 성능 저하 발생 케이스

### MCP (Model Context Protocol)

```
┌─────────────┐         ┌─────────────┐
│  LLM Agent  │◀───────▶│ MCP Server  │
│             │   MCP   │  (Tools)    │
└─────────────┘         └─────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         ▼                    ▼                    ▼
    ┌─────────┐         ┌─────────┐         ┌─────────┐
    │Filesystem│         │Database │         │  API    │
    └─────────┘         └─────────┘         └─────────┘
```

---

## 6. Coding Agent

### 핵심 프로젝트 및 논문

| 프로젝트 | 연도 | SWE-Bench 성능 | 핵심 특징 |
|----------|------|----------------|----------|
| [OpenHands](https://arxiv.org/abs/2407.16741) | 2024 | 72% (Verified) | CodeAct 아키텍처 |
| SWE-Agent | 2024 | 65% | 편집 전문 |
| Devin | 2024 | - | 상용, 자율 실행 |
| Claude Code | 2025 | - | Anthropic 공식 |

### OpenHands CodeAct 아키텍처

```
┌─────────────────────────────────────────────────────────────┐
│                    CodeAct Agent                             │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐                   │
│  │  Web Browsing   │  │  Code Editing   │                   │
│  │   Specialist    │  │   Specialist    │                   │
│  └────────┬────────┘  └────────┬────────┘                   │
│           │                    │                             │
│           └────────┬───────────┘                             │
│                    ▼                                         │
│           ┌─────────────────┐                                │
│           │  Main CodeAct   │                                │
│           │     Agent       │                                │
│           └────────┬────────┘                                │
│                    │                                         │
│  ┌─────────────────┼─────────────────┐                      │
│  ▼                 ▼                 ▼                      │
│ Bash            IPython           Browser                   │
│ Runtime         Runtime           Runtime                   │
└─────────────────────────────────────────────────────────────┘
```

### SWE-EVO 벤치마크 (2025)

**기존 SWE-Bench Verified vs SWE-EVO**:
- SWE-Bench Verified: GPT-4 65% 해결
- SWE-EVO: GPT-5도 **19-21%만 해결**

**SWE-EVO 특징**:
- 장기간 소프트웨어 진화 시나리오
- 실제 프로덕션 환경 복잡도

---

## 7. Multi-Agent 시스템

### 핵심 프레임워크

| 프레임워크 | 특징 | 주요 도메인 |
|-----------|------|------------|
| [AutoGen](https://arxiv.org/pdf/2308.08155) | 대화 기반 다중 Agent | 범용 |
| [CrewAI](https://www.crewai.com/) | 역할 기반 팀 구성 | 마케팅, 채용 |
| [CAMEL](https://arxiv.org/abs/2303.17760) | Inception prompting | 데이터 생성 |
| [MetaGPT](https://arxiv.org/abs/2308.00352) | SOP 기반 협업 | 소프트웨어 개발 |

### Multi-Agent Collaboration Survey (2025)

```
Collaboration Dimensions
├── Actors (참여자)
│   ├── Homogeneous: 동일 Agent
│   └── Heterogeneous: 다양한 전문 Agent
│
├── Types (유형)
│   ├── Cooperation: 협력
│   ├── Competition: 경쟁
│   └── Coopetition: 협력+경쟁
│
├── Structures (구조)
│   ├── Peer-to-peer: 동등
│   ├── Centralized: 중앙 집중
│   └── Distributed: 분산
│
└── Coordination (조율)
    ├── Role-based: 역할 기반
    ├── Model-based: 모델 기반
    └── Market-based: 시장 기반
```

### 도전 과제

1. **Coordination Overhead**: Agent 간 상호작용 관리 복잡성
2. **Resource Inefficiency**: 모든 작업에 고성능 모델 사용 낭비
3. **Planning Limitations**: 효과적인 작업 분해 어려움

---

## 8. 자기 반성 (Self-Reflection)

### 핵심 논문

| 논문 | 연도 | 핵심 내용 |
|------|------|----------|
| [Reflexion](https://arxiv.org/abs/2303.11366) | 2023 | Verbal Reinforcement Learning |
| [Self-Reflection Effects](https://arxiv.org/abs/2405.06682) | 2024 | Self-reflection 효과 분석 |
| [MAR: Multi-Agent Reflexion](https://arxiv.org/html/2512.20845) | 2025 | Multi-agent 확장 |

### Reflexion 아키텍처

```
┌─────────────────────────────────────────────────────────────┐
│                    Reflexion Loop                            │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   Trial 1:                                                   │
│   ┌────────┐    ┌────────┐    ┌────────┐                   │
│   │ Actor  │───▶│  Env   │───▶│Evaluator│                   │
│   └────────┘    └────────┘    └───┬────┘                   │
│        ▲                          │ feedback                │
│        │    ┌─────────────────────▼─────┐                   │
│        │    │     Self-Reflection        │                   │
│        │    │ "I failed because..."      │                   │
│        │    └─────────────┬─────────────┘                   │
│        │                  │                                  │
│        │    ┌─────────────▼─────────────┐                   │
│        │    │    Memory Buffer           │                   │
│        │    │ [reflection_1, ...]        │                   │
│        └────┴───────────────────────────┘                   │
│                                                              │
│   Trial 2: (with reflection context)                         │
│   ...improved performance...                                 │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 주요 발견

**Self-Reflection 효과 (2024)**:
- 문제 해결 성능 **유의미하게 향상** (p < 0.001)
- 무한 루프 방지에 효과적

**Degeneration-of-Thought 문제**:
- LLM이 동일한 잘못된 추론 반복
- 페르소나 변경해도 근본 추론 패턴 유지
- **해결책**: MAR (Multi-Agent Reflexion)

### MAR 결과 (2025)

| 벤치마크 | Reflexion | MAR | 개선 |
|---------|-----------|-----|------|
| HotPotQA | 44% | 47% | +3pt |
| HumanEval | 76.4% | 82.6% | +6.2pt |

---

## 9. 구현 우선순위

### 즉시 구현 (High Priority)

1. **A-MEM 스타일 메모리** 
   - Zettelkasten 기반 연결 메모리
   - 85-93% 토큰 절감
   - `crates/Layer3-agent/src/strategy/memory.rs` 확장

2. **MAR (Multi-Agent Reflexion)**
   - 다양한 페르소나로 비평
   - Degeneration-of-thought 해결
   - `crates/Layer3-agent/src/variant/` 새 변형 추가

3. **GoalAct Planning**
   - 전역 계획 + 계층적 실행
   - Skill 기반 분해
   - `crates/Layer3-agent/src/strategy/planning.rs` 확장

### 중기 구현 (Medium Priority)

4. **CodeAct 패턴**
   - OpenHands 스타일 코드 실행
   - 전문가 Agent (Web, Code) 통합
   - Layer3 새 variant

5. **MCP 완전 지원**
   - Tool provider 표준화
   - `crates/Layer2-core/src/mcp/` 확장

6. **Adaptive Tool Calling**
   - JSON 제약 완화 옵션
   - 자연어 tool 호출 지원

### 장기 연구 (Low Priority)

7. **Multi-Agent Orchestration**
   - AutoGen/CrewAI 스타일 협업
   - Agent 간 통신 프로토콜

8. **Tree Search RL**
   - GRPO 기반 강화학습
   - Agent 정책 최적화

---

## References

### Surveys
- [Agentic Large Language Models, a survey](https://arxiv.org/html/2503.23037v3)
- [Multi-Agent Collaboration Mechanisms: A Survey](https://arxiv.org/html/2501.06322v1)
- [Understanding the Planning of LLM Agents](https://arxiv.org/abs/2402.02716)

### Reasoning
- [ReAct: Synergizing Reasoning and Acting](https://arxiv.org/abs/2210.03629)
- [Tree of Thoughts](https://arxiv.org/abs/2305.10601)
- [Chain-of-Thought Prompting](https://arxiv.org/abs/2201.11903)

### Memory
- [A-MEM: Agentic Memory](https://arxiv.org/abs/2502.12110)
- [Mem0](https://arxiv.org/pdf/2504.19413)
- [MemGPT](https://arxiv.org/abs/2310.08560)

### Coding
- [OpenHands](https://arxiv.org/abs/2407.16741)
- [SWE-EVO Benchmark](https://arxiv.org/html/2512.18470v2)

### Self-Reflection
- [Reflexion](https://arxiv.org/abs/2303.11366)
- [MAR: Multi-Agent Reflexion](https://arxiv.org/html/2512.20845)

---

*Last Updated: 2025-02*
*ForgeCode Research Team*
