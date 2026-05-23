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

`spel-spike/` is an isolated Cargo package outside the main workspace (note the empty `[workspace]` table at `spel-spike/Cargo.toml`). It hosts a SPEL-shape mirror of the Token program at `spel-spike/methods/guest/src/bin/token.rs`. The `spel generate-idl` CLI is invoked against the sidecar; the emitted IDL is committed at `artifacts/token.idl.spel.json`.

Crucially, **the sidecar source is never compiled** by the main build or the SPEL CLI. `spel generate-idl` does syntax-level parsing only (it reads function signatures and `#[account(...)]` attributes via `syn`). The Cargo.toml exists only so the directory is a self-documenting unit and a future contributor could try compiling it if SPEL's workspace-integration story improves.

This is the same workaround taken by the prior community submission ([PR #57](https://github.com/logos-co/lambda-prize/pull/57), Tranquil-Flow), which shipped both a sidecar-emitted IDL **and** a hand-authored canonical IDL — the bar this solution matches.

## Scaffold ↔ real program mapping

| Real program (`programs/token/`) | Sidecar mirror (`spel-spike/methods/guest/src/bin/token.rs`) |
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
spel -- generate-idl spel-spike > artifacts/token.idl.spel.json
```

Both files (`artifacts/token.idl.spel.json` and `artifacts/token.idl.json`) are committed and reviewed together.
