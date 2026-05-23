# `spel-sidecar` — sidecar for SPEL IDL emission

This directory exists for **one purpose**: hold a SPEL-shape mirror of the LP-0013 Token program so that `artifacts/token.idl.spel.json` can be produced by the real `spel generate-idl` toolchain rather than hand-authored.

It is **not** a Cargo workspace member of the main `logos-execution-zone` workspace. The empty `[workspace]` table in `Cargo.toml` deliberately breaks it out. See `docs/SPEL_STATUS.md` for the full rationale (short version: SPEL's git pin of `nssa_core v0.2.0-rc3` collides with this repo's local `nssa_core v0.1.0`; the sidecar avoids the collision by living outside the workspace).

The sidecar's source file is **not** compiled by anything in the main build. `spel generate-idl` only **parses** it (syntax-level only) to extract instruction signatures and `#[account_type]` shapes.

## Layout

```
spel-sidecar/
├── Cargo.toml                              # [workspace] + isolated package; never built
├── README.md                                # this file
└── methods/
    └── guest/
        └── src/
            └── bin/
                └── token.rs                 # SPEL mirror of the Token program
```

The `methods/guest/src/bin/` path is the SPEL CLI's default discovery location, so `spel generate-idl spel-sidecar` finds the source automatically.

## Regenerating the IDL

Prerequisites: install the SPEL CLI once (host-only build, no riscv32 target needed):

```bash
cargo install --git https://github.com/logos-co/spel --tag v0.4.0 spel
```

From the repo root:

```bash
spel -- generate-idl spel-sidecar > artifacts/token.idl.spel.json
```

The output is committed at `artifacts/token.idl.spel.json` (10 KB, 11 instructions, 6 account types).

## Companion IDL

`artifacts/token.idl.json` (hand-authored) is the **canonical** IDL — it conforms to the `SpelIdl` schema and carries shapes the v0.4.0 CLI does not yet emit (e.g. the `errors` table for `ApprovalError`). The SPEL-emitted IDL here is the **provenance** companion. Both are kept in `artifacts/` and both are listed in the solution PR.

## When to update this file

Any change to:

- the `Instruction` enum in `programs/token/core/src/lib.rs` (add / remove / rename a variant, change an argument)
- the `TokenDefinition` / `TokenHolding` / `TokenMetadata` enum or struct shapes in the same file

…must be mirrored in `methods/guest/src/bin/token.rs` and then `artifacts/token.idl.spel.json` regenerated and re-committed. The mirror is a maintenance liability — see `docs/SPEL_STATUS.md` for why we accept it.
