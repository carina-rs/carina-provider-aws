# CLAUDE.md

This file provides guidance to Claude Code when working with the carina-provider-aws repository.

## Repository Overview

This is the AWS provider for [Carina](https://github.com/carina-rs/carina), split out as a standalone repository. It depends on carina-core, carina-aws-types, carina-plugin-sdk, and carina-provider-protocol via git dependencies from the main carina repository.

## Build and Test Commands

```bash
# Build
cargo build

# Run all tests
cargo test

# Build WASM target
cargo build -p carina-provider-aws --target wasm32-wasip2 --release

# Run clippy
cargo clippy -- -D warnings

# Format check
cargo fmt --check
```

### With AWS Credentials

```bash
aws-vault exec <profile> -- cargo test
```

## Crate Structure

- **carina-provider-aws**: The AWS provider implementation. Builds as both a native binary and a WASM component.
- **carina-codegen-aws**: Code generator that produces resource definitions from AWS Smithy models.
- **carina-smithy**: Smithy 2.0 JSON AST parser used by the code generator.

## Dependencies on carina (main repo)

This repository depends on crates from `github.com/carina-rs/carina`:
- `carina-core` — Core types, parser, traits
- `carina-aws-types` — AWS-specific type definitions
- `carina-plugin-sdk` — Plugin SDK for building providers
- `carina-provider-protocol` — Protocol definitions for provider communication

These are specified as `git` dependencies in `Cargo.toml`. For local development, you can override them in `.cargo/config.toml`:

```toml
[patch."https://github.com/carina-rs/carina"]
carina-core = { path = "../carina/carina-core" }
carina-aws-types = { path = "../carina/carina-aws-types" }
carina-plugin-sdk = { path = "../carina/carina-plugin-sdk" }
carina-provider-protocol = { path = "../carina/carina-provider-protocol" }
```

## Code Generation

The `carina-codegen-aws` crate generates resource definitions from AWS Smithy JSON models:

```bash
cargo run -p carina-codegen-aws -- <smithy-model-path>
```

## Git Workflow

### Worktree-Based Development

**IMPORTANT: Use `git wt` (NOT `git worktree`).** `git wt` is a separate tool with its own syntax.

```bash
git wt <branch-name> main    # Create worktree
git wt                       # List worktrees
git wt -d <branch-name>      # Delete worktree (from main worktree)
```

## Code Style

- **Commit messages**: Write in English
- **Code comments**: Write in English
