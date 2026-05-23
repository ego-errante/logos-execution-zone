# SPEL IDL — status disclosure for LP-0013

This solution ships **two** IDL files at `artifacts/`:

| File | Author | Purpose |
|---|---|---|
| `artifacts/token.idl.spel.json` | `spel generate-idl` (real SPEL toolchain, v0.4.0) | **Provenance** — proves the program shape parses through SPEL's macro grammar |
| `artifacts/token.idl.json` | Hand-authored against the `SpelIdl` schema in `spel-framework-core` | **Completeness** — includes shapes the v0.4.0 CLI does not yet emit (e.g. the `errors` table for `ApprovalError::{Unauthorized, Renounced}`) |

Both files describe the same Token program. The hand-authored canonical IDL is the source of truth for tooling integration; the SPEL-emitted IDL is the source of truth for "did this program go through the SPEL framework, end-to-end."

## Why two artifacts?

The straightforward path — annotate `program_methods/guest/src/bin/token.rs` directly with `#[spel_framework::lez_program]`, depend on `spel-framework` from the workspace, and let the macro emit the IDL during the normal guest build — does **not** compile in this checkout. The reason is a dep-graph collision between two versions of the same crate, plus a downstream cross-compile failure that the collision makes structurally difficult to fix:

1. **`nssa_core` version split.** This repo's workspace uses a local path dep on `nssa_core` v0.1.0 (`programs/token/core` → `nssa_core = { path = "nssa/core" }`). The `spel-framework` crate, in every released tag, pins `nssa_core` as a git dep to tag `v0.2.0-rc3`. Cargo accepts both — they compile as two separate crates with the same name. The `#[lez_program]` proc macro emits dispatcher code that writes path-literal references to `::nssa_core::account::AccountWithMetadata`; depending on which version cargo chose for the macro-invoking crate, either the dispatcher's internal call sites or the user-written handlers reject the input. There is no expansion-time switch — one `nssa_core` universe per macro invocation.

2. **`ring` cc-rs failure in the riscv32 guest closure.** `nssa_core v0.2.0-rc3`'s `host` feature transitively pulls in `k256` → `ring` → `cc-rs`. When the guest crate is cross-compiled to `riscv32im-risc0-zkvm-elf`, `cc-rs` reads the macOS arm64 host triple and feeds host-specific flags to the C compiler, which the riscv32 toolchain rejects. The `[patch]`-back-to-v0.1.0 workaround is a coin flip — it can succeed but is sensitive to API drift and feature-flag mismatches between v0.1.0 and v0.2.0-rc3 internals.

The structural fix — upgrade the entire workspace to `nssa_core v0.2.0-rc3` — would also invalidate every other LP-0013 commit on this branch because it is a different LEZ snapshot. Mid-prize-cycle workspace upgrades are not safe within the available time budget.

## The sidecar approach

`spel-sidecar/` is an isolated Cargo package outside the main workspace (note the empty `[workspace]` table at `spel-sidecar/Cargo.toml`). It hosts a SPEL-shape mirror of the Token program at `spel-sidecar/methods/guest/src/bin/token.rs`. The `spel generate-idl` CLI is invoked against the sidecar; the emitted IDL is committed at `artifacts/token.idl.spel.json`.

Crucially, **the sidecar source is never compiled** by the main build or the SPEL CLI. `spel generate-idl` does syntax-level parsing only (it reads function signatures and `#[account(...)]` attributes via `syn`). The Cargo.toml exists only so the directory is a self-documenting unit and a future contributor could try compiling it if SPEL's workspace-integration story improves.

This is the same workaround taken by the prior community submission ([PR #57](https://github.com/logos-co/lambda-prize/pull/57), Tranquil-Flow), which shipped both a sidecar-emitted IDL **and** a hand-authored canonical IDL — the bar this solution matches.

## Scaffold ↔ real program mapping

| Real program (`programs/token/`) | Sidecar mirror (`spel-sidecar/methods/guest/src/bin/token.rs`) |
|---|---|
| `Instruction::Transfer` | `pub fn transfer(...)` |
| `Instruction::NewFungibleDefinition` | `pub fn new_fungible_definition(...)` |
| `Instruction::NewFungibleDefinitionWithAuthority` | `pub fn new_fungible_definition_with_authority(...)` |
| `Instruction::NewDefinitionWithMetadata` | `pub fn new_definition_with_metadata(...)` |
| `Instruction::InitializeAccount` | `pub fn initialize_account(...)` |
| `Instruction::Burn` | `pub fn burn(...)` |
| `Instruction::Mint` | `pub fn mint(...)` |
| `Instruction::MintWithAuthority` (LP-0013) | `pub fn mint_with_authority(...)` |
| `Instruction::PrintNft` | `pub fn print_nft(...)` |
| `Instruction::RotateAuthority` (LP-0013) | `pub fn rotate_authority(...)` |
| `Instruction::RevokeAuthority` (LP-0013) | `pub fn revoke_authority(...)` |
| `TokenDefinition` enum | `#[account_type] pub enum TokenDefinition` |
| `TokenHolding` enum | `#[account_type] pub enum TokenHolding` |
| `TokenMetadata` struct | `#[account_type] pub struct TokenMetadata` |
| `MetadataStandard` enum | `#[account_type] pub enum MetadataStandard` |
| `NewTokenDefinition` enum | `#[account_type] pub enum NewTokenDefinition` |
| `NewTokenMetadata` struct | `#[account_type] pub struct NewTokenMetadata` |

### Acknowledged limitations of the mirror

- **`AccountId` → `[u8; 32]`.** The real handlers use `nssa_core::account::AccountId` (a newtype around `[u8; 32]`); the mirror uses the raw byte array since the sidecar's `nssa_core` is a different crate from the workspace's `nssa_core` and importing it would defeat the isolation. The wire layout is identical.
- **`Option<AccountId>` → `[u8; 32]`.** Same reason; the canonical hand-authored IDL captures the `Option` shape faithfully.
- **`Box<NewTokenMetadata>` → `NewTokenMetadata`.** The Box wrapper is a Rust-level size optimization; the wire shape is the inner type.
- **`ApprovalError` table missing from the SPEL emission.** SPEL v0.4.0's CLI does not yet emit a top-level `errors` table. The hand-authored canonical IDL covers this (with `Unauthorized` and `Renounced` variants attributed to the `lez-approval` crate).
- **Account `signer` / `mut` / `init` annotations.** The mirror best-effort reflects the real handlers' authorization semantics (e.g. `mint_with_authority`'s third account is `#[account(signer)]` because the real handler panics with `Unauthorized` if `authority.is_authorized` is false). The canonical IDL is the source of truth where the two disagree.

## Reproducibility

Install the SPEL CLI once (host-only build, no riscv32 target required):

```bash
cargo install --git https://github.com/logos-co/spel --tag v0.4.0 spel
```

Regenerate the IDL from the repo root:

```bash
spel -- generate-idl spel-sidecar > artifacts/token.idl.spel.json
```

Both files (`artifacts/token.idl.spel.json` and `artifacts/token.idl.json`) are committed and reviewed together.

## What surprised us during the sidecar build

Honest engineering accounting — paper cuts and recalibrations encountered while implementing the sidecar, in case any are useful to a reviewer or to the next builder hitting the same wall.

### 1. The SPEL CLI installed cleanly on macOS arm64

We expected `cargo install --git https://github.com/logos-co/spel --tag v0.4.0 spel` to fail. The spel CLI depends on `nssa_core v0.2.0-rc3` with `features = ["host"]`, which transitively pulls in `ring` → `cc-rs` — the same chain that breaks our riscv32 guest build.

It installed in ~3 minutes, exit 0.

The recalibration: `cc-rs` fails on `riscv32im-risc0-zkvm-elf` because that's a cross-compile target with no native C toolchain that understands macOS arm64 host flags (`-arch arm64`, `-mmacosx-version-min`). For a *native* host build, `cc-rs` does exactly what it's designed to do — pick the host compiler with host flags. `ring` builds natively on arm64-darwin all the time. The failure mode is target-specific, not crate-specific. We had been thinking "ring is in the dep tree → ring fails," not "ring + cross-compile-to-riscv32 → ring fails." This is why the Reproducibility section above flags "host-only build, no riscv32 target required" as a meaningful caveat.

### 2. `#[account]` without parens isn't supported in v0.4.0

Rust attribute grammar allows both `#[attr]` and `#[attr(...)]`. We expected bare `#[account]` to be the shorthand for "default account, no constraints," which we needed for `initialize_account`, `burn`, and `mint_with_authority` where the first account is a read-only definition with no `mut` / `signer` / `init` annotations. First run failed with:

```
Error: Parse error: expected attribute arguments in parentheses: #[account(...)]
```

SPEL's attribute parser uses `syn::Attribute::parse_args` (or equivalent), which assumes the `(...)` is there — no "no args" branch. Fix: write `#[account()]` with empty parens. Cosmetically ugly, valid Rust, 4-character diff per occurrence.

### 3. Workspace-walk warning when the source file sits outside SPEL's expected layout

First invocation as `spel -- generate-idl spel-sidecar/src/lib.rs` emitted:

```
⚠️  workspace at '' has no matching member for 'spel-sidecar/src/lib.rs'; searching all subdirectories
```

The CLI walks up from the input file looking for a `Cargo.toml` and tries to verify the input belongs to a workspace member crate — not for compilation, but so it can scan path deps for `#[account_type]` declarations. With `spel-sidecar/Cargo.toml` declaring `[workspace]` (empty), the workspace member list is empty, the input file isn't in it, and the CLI falls back to subdirectory scanning. Cosmetic warning, correct output.

Fix: move source to `spel-sidecar/methods/guest/src/bin/token.rs` — the SPEL-idiomatic discovery path — and invoke as `spel -- generate-idl spel-sidecar` (directory, not file). Now matches the layout the SPEL README documents as canonical. This was a "your fault for not matching the convention" surprise rather than a SPEL bug.

### 4. v0.4.0 doesn't emit a top-level `errors` table

The `SpelIdl` schema in `spel-framework-core/src/idl.rs` has fields for `version`, `name`, `spec`, `metadata`, `instructions`, `accounts`, `types`, `errors`, `instruction_type`. The hand-authored canonical IDL fills all 9. The v0.4.0 CLI emits only `version`, `name`, `instructions`, `accounts`. No `errors`, no `types` as a separate field (types are inlined into account variants), no `spec`, no `metadata`, no `instruction_type`.

Specifically: the `errors` table — which would list `ApprovalError::{Unauthorized, Renounced}` with their codes — is absent. The schema field exists in `SpelIdl`; the macro/CLI just doesn't have a collection pass that walks the source for error-enum declarations. Schema is forward-declared; emitter hasn't caught up.

This is why the two-IDL strategy isn't only a workaround for the workspace-integration blockers — even if we'd successfully integrated SPEL directly into the workspace, we'd still need the hand-authored IDL to cover the `errors` table. The dep-graph collision forced us into the pattern; the schema gap means the pattern was the right shape anyway.

### Headline lesson

The two stacked blockers documented above (`nssa_core` collision + `ring`/`cc-rs` cross-compile) are real and both still apply to workspace-integrated SPEL. The sidecar approach side-steps both, but it does so by *not testing them* — the sidecar never compiles, never cross-compiles, never integrates. We did not solve the original problems; we routed around them, and the routing turned out to be both cheap and well-precedented (PR #57). This document exists so reviewers can see the routing for what it is rather than infer it.
