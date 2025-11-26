# Repository Guidelines

## Project Structure & Module Organization
Actix entry point `src/main.rs` wires `config.rs`, `state.rs`, and `shutdown.rs`, while feature code sits in `src/auth` (token validation), `src/handlers` (HTTP/WebSocket routes), `src/ws` (session and message types), and `src/redis` (pub/sub client). Shared errors live in `src/error.rs`. Rust integration specs live under `tests/`, asyncio system tests in `test_wsproxy.py`, manual agents in `examples/`, and container tooling in `Dockerfile` plus the `docker-compose*.yml` stack.

## Build, Test, and Development Commands
- `cargo build` – compile wsproxy and dependencies.
- `HOST=0.0.0.0 PORT=4040 REDIS_URL=redis://127.0.0.1:6379 cargo run` – start the proxy against local Redis.
- `cargo test` – run unit and integration suites in `tests/`.
- `cargo fmt && cargo clippy -- -D warnings` – enforce formatting and lint gates before pushing.
- `uv sync` – install Python integration-test deps declared in `pyproject.toml`.
- `uv run test_wsproxy.py` – execute asyncio end-to-end tests (proxy and Redis must already be running).
- `docker compose up wsproxy redis` – boot the containerized stack described in `docker-compose.yml`.

## Coding Style & Naming Conventions
Use Rust 1.70+ with `rustfmt` defaults (4-space indent, trailing commas) and run `cargo fmt` before committing. Keep modules, files, and Redis channels `snake_case`, types/enums `PascalCase`, and env/constants `SCREAMING_SNAKE_CASE`. Return `Result` with `?`, extend the existing `thiserror` enums in `src/error.rs`, and rely on `tracing` macros instead of ad-hoc printlns.

## Testing Guidelines
Rust tests in `tests/` follow the `<behavior>_flow.rs` pattern with `#[tokio::test]`; seed Redis keys like `session:{id}:auth` through helpers and clean them afterward. Assert on HTTP status and round-trip payloads similar to `test_downstream_message_delivery`. Python orchestration tests rely on uv—run `uv run test_wsproxy.py` against a running proxy and Redis to validate WebSocket ↔ Redis behavior before submitting. Capture any manual scenarios in `examples/README.md`.

## Commit & Pull Request Guidelines
Recent history favors short, imperative commit subjects (for example, “Add Python integration test suite with asyncio and uv”). Mirror that tone, keep each commit focused, and include doc or test updates alongside code changes. Pull requests must explain the problem, configuration deltas, and risks, reference related issues, list executed commands, and attach screenshots or logs whenever behavior changes.

## Security & Configuration Tips
Session tokens and channels follow `session:{id}:*`; never log raw tokens or Redis payloads. Keep environment overrides in `.env` (modelled after `.env.example`) and inject them via `HOST`, `PORT`, `REDIS_URL`, and friends. When running outside localhost, ensure TLS termination happens upstream and that Docker Compose secrets or orchestration vars provide Redis credentials.
