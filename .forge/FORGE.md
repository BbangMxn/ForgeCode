# ForgeCode 프로젝트 설정

## 빌드 & 테스트

```bash
cargo build --release
cargo test --workspace
```

## 실행

```bash
./target/release/forge --provider ollama --model qwen3:8b -p "Hello"
```
