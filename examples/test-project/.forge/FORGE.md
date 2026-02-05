# Test Project

ForgeCode Task 시스템을 테스트하기 위한 프로젝트입니다.

## 테스트 시나리오

1. **서버-클라이언트 테스트**: 서버 시작 → 클라이언트 실행 → 결과 확인
2. **성능 테스트**: 라이브러리 함수 성능 측정
3. **유닛 테스트**: `cargo test` 실행

## 사용 예시

```bash
# 서버 시작
cargo run --bin server

# 클라이언트 실행 (다른 터미널)
cargo run --bin client

# 테스트
cargo test
```
