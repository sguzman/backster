# Backster

Backster is a Rust backtesting and optimization project for running repeatable strategy experiments against tabular market data.

## Intent

Provide a config-driven command-line workflow for strategy backtests and optimization runs without forcing the experiment logic into notebooks or one-off scripts.

## Ambition

Inferred from the module layout (`strategy`, `optimizer`, `report`, `stats`, `cache`) and the `strats/` + `wls/` folders, the longer-term ambition appears to be a reusable quantitative research bench that can compare strategies, tune parameters, and accumulate reproducible experiment artifacts.

## Current Status

The current codebase already has separate backtest and optimize modes, TOML config loading, repeated-run statistics, tracing, and data/cache plumbing. It still reads like an actively evolving research tool rather than a polished end-user product.

## Core Capabilities Or Focus Areas

- Config-driven execution with a selected runtime mode.
- Backtest execution for a chosen data/strategy configuration.
- Optimization runs across parameterized search space.
- Repeated-run statistics via `--n-times` for variance analysis.
- Tracing-based console diagnostics.

## Project Layout

- `src/`: Rust source for the main crate or application entrypoint.
- `strats/`: strategy definitions, presets, or experiment assets used by research runs.
- `wls/`: supporting whitelist or selection data consumed by configured workflows.
- `Cargo.toml`: crate or workspace manifest and the first place to check for package structure.

## Setup And Requirements

- Rust toolchain with the 2024 edition compiler.
- Project-specific input data and a TOML configuration file.
- Any local strategy or whitelist assets expected by the chosen config.

## Build / Run / Test Commands

```bash
cargo build
cargo test
cargo run -- --config path/to/config.toml
```

## Notes, Limitations, Or Known Gaps

- The CLI expects a config file; there is no zero-config demo path checked into the project root.
- This repo appears optimized for private research workflows, so some strategy/data assumptions likely live outside the source tree.

## Next Steps Or Roadmap Hints

- Stabilize a canonical sample config and dataset for reproducible onboarding.
- Expand reporting and artifact output so experiment runs are easier to compare over time.
