# Contributing to Nooki

Thanks for helping improve Nooki.

## Before opening a change

- Keep the community edition local-only.
- Do not commit API keys, private keys, Minecraft worlds, backups, or generated server files.
- Use `pnpm` for frontend dependencies and scripts.
- Keep Windows x64 as the supported target unless a change explicitly adds and tests another platform.

## Development checks

Install the requirements listed in the README, then run:

```powershell
pnpm install
pnpm build
pnpm test
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Explain the user-facing behavior and testing performed in each pull request. Keep unrelated changes in separate pull requests.

By contributing, you agree that your contribution is licensed under the MIT License.
