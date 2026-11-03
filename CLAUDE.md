# CLAUDE.md

This file provides guidance to Claude Code when working with the carina-provider-aws repository.

## Repository Overview

This is the AWS provider for [Carina](https://github.com/carina-rs/carina), split out as a standalone repository. It depends on carina-core, carina-plugin-sdk, and carina-provider-protocol via git dependencies from the main carina repository. `carina-aws-types` lives in this repository (a local copy, not shared from the main repo).

## Build and Test Commands

```bash
# Build
cargo build

# Run all tests
cargo test

# Build WASM target
cargo build -p carina-provider-aws --target wasm32-wasip2 --release

# Run clippy (same command as CI; --all-targets covers tests, and
# clippy.toml enforces read-state construction)
cargo clippy --workspace --all-targets -- -D warnings

# Format check
cargo fmt --check
```

### AWS Authentication

WASM providers use base credentials resolved by the Carina host's standard AWS
credential chain. Named profiles and SSO work through the host; select a profile
with `AWS_PROFILE`. The host supplies credentials and expiry through the typed
`aws-credentials` import and refreshes them when needed. The provider uses these
base credentials for any provider-level `assume_role` call.

Carina never starts an interactive login. When an SSO session has expired, run
`aws sso login --profile <profile>` and retry. Service regions remain explicit
provider configuration, defaulting to `ap-northeast-1`.

Native builds keep the AWS SDK's default credential chain. For native tests that
require AWS credentials:

```bash
aws-vault exec <profile> -- cargo test
```

## Crate Structure

- **carina-provider-aws**: The AWS provider implementation. Builds as both a native binary and a WASM component.
- **carina-codegen-aws**: Code generator that produces resource definitions from AWS Smithy models.
- **carina-smithy**: Smithy 2.0 JSON AST parser used by the code generator.
- **carina-aws-types**: Shared AWS-specific types, guarded SDK handles, `HostCredentialsProvider`, and `GuardedAssumeRoleProvider`. This crate lives in this repo; `carina-provider-awscc` consumes it by pinning this repository.

## Dependencies on carina (main repo)

This repository depends on crates from `github.com/carina-rs/carina`:
- `carina-core` — Core types, parser, traits
- `carina-plugin-sdk` — Plugin SDK for building providers
- `carina-provider-protocol` — Protocol definitions for provider communication

These are specified as `git` dependencies in `Cargo.toml`. For local development, you can override them in `.cargo/config.toml`:

```toml
[patch."https://github.com/carina-rs/carina"]
carina-core = { path = "../carina/carina-core" }
carina-plugin-sdk = { path = "../carina/carina-plugin-sdk" }
carina-provider-protocol = { path = "../carina/carina-provider-protocol" }
```

`carina-aws-types` is **not** a main-repo dependency — it is a local crate in
this repository (`carina-provider-aws/Cargo.toml` references it as
`{ path = "../carina-aws-types" }`), so it needs no patch entry.

## AWS SDK Access

`carina-aws-types` owns all direct `aws-sdk-*` and `aws-config` dependencies.
Provider crates must not add either dependency, including in dev or target
sections. Use `carina_aws_types::sdk::<service>` for SDK types, operations, errors, and
primitives. The facade never re-exports raw `Client`, `Config`, or `config`;
compile-fail doctests enforce that boundary. Its service handles are constructed
only with a permanent `ResendGuard` and the service's idempotency lookup.
S3 Express session authentication is disabled. A rejecting Express credentials
provider also prevents an unguarded internal CreateSession client when an
operation overrides that flag.

Generated provider wrappers keep the closed operation API and stable token
handling. Add operation decisions to `carina-codegen-aws/src/resend_safety.rs`
and regenerate with `./scripts/generate-provider.sh`. KMS reference routing uses
the same facade factories. Provider-level AssumeRole uses
`carina_aws_types::GuardedAssumeRoleProvider`, whose STS requests are guarded as
non-idempotent and retain the base credentials and expiration-based refresh path.
Concurrent loads share one pending AssumeRole request, including after a caller
is cancelled; completed results are left to the consuming SDK identity cache.
Shared configuration loading uses `carina_aws_types::load_sdk_config`; its API
defaults to `HostCredentialsProvider` on WASM when no override is supplied,
returns only `aws_types::SdkConfig`, and never exposes aws-config's loaders or
credential-provider builders. Use `aws-types` for `SdkConfig` and `Region`,
`aws-smithy-runtime-api` for `BehaviorVersion`, and `aws-smithy-runtime` for
`IdentityCache`.

Client safety comes from dependency visibility and permanent interceptors.
The hand-maintained `clippy.toml` governs read-state construction and is not a
codegen output.

## Code Generation

The `carina-codegen-aws` crate generates resource definitions from AWS Smithy JSON models:

```bash
cargo run -p carina-codegen-aws -- <smithy-model-path>
```

## Record measured AWS behaviour in the code, with date and region

Provider work regularly turns on how a specific AWS API actually behaves rather
than on what its documentation or Smithy model says: which fields a read returns,
whether an update accepts a change or silently ignores it, how a value is spelled
back, which changes force replacement. Verify these against real AWS
(`aws-vault exec <profile> -- ...`) before designing around them.

When an observation produces a code path — an override, a special-case branch, a
field that must be stripped or synthesised, a value spelling that has to be
translated — record the observation in a comment at that code path. Note what was
run, the **region**, the **date**, and which directions were checked.

```rust
// UpdateFunctionConfiguration accepts removing this block but the read-back is
// unchanged, so a presence toggle must force replacement rather than an
// in-place update. Measured us-east-1, 2026-08-12, both directions
// (add -> ValidationException; remove -> accepted but read-back unchanged).
```

The reason is re-measurement, not provenance. AWS behaviour changes, and a branch
whose justification lives only in a PR thread becomes an incantation nobody dares
touch. With a date and a region, a future reader can decide whether the
observation is still current and re-run it. Without them, the only safe move is
to leave the branch alone forever.

Applies equally to the codegen layer: a `resource_type_overrides()` entry or a
hand-written deviation from the Smithy model that exists because real AWS
disagrees with the model needs the same comment, since the generated output alone
cannot explain itself.

## Git Workflow

### Worktree-Based Development

```bash
git worktree add .worktrees/<branch-name> -b <branch-name> main   # Create worktree
git worktree list                                                  # List worktrees
git worktree remove .worktrees/<branch-name>                       # Delete worktree (from the main worktree)
```

## Code Style

- **Commit messages**: Write in English
- **Code comments**: Write in English
