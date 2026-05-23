# variable-supply

LP-0013 example integration: a **fungible token with a rotating mint authority**.

This is the canonical "ongoing issuance with hand-off" pattern, suitable for
governance-managed tokens (DAO key rotation), credential/reward systems where
the issuing role moves between operators, or any setup that needs to retire one
minter and promote another without breaking the supply trail.

## What it demonstrates

The example runs a complete authority-rotation lifecycle against a standalone
sequencer:

1. Creates six public accounts (definition, supply, two authorities `auth_a`
   and `auth_b`, two holders `holder_a` and `holder_b`).
2. Calls `NewFungibleDefinitionWithAuthority` to define a token named
   `variable-supply-demo` with `auth_a` as the initial mint admin.
3. Calls `MintWithAuthority` via `auth_a` to mint **1000** to `holder_a`.
4. Calls `RotateAuthority` to hand the mint admin role from `auth_a` to `auth_b`.
5. Calls `MintWithAuthority` via `auth_b` to mint **500** to `holder_b`.
6. Attempts a third `MintWithAuthority` via `auth_a` and observes the sequencer
   reject it (`auth_a` is no longer the active authority — `Authority.gate`
   panics with `ApprovalError::Unauthorized`).
7. Reads and asserts both holders' balances (1000 and 500 respectively).

## Prerequisites

You need a standalone LEZ sequencer reachable at the address in your wallet
config (default `127.0.0.1:3040`), with the Token program already deployed and
the wallet storage initialized.

The simplest way to set this up is via the repo-root `demo.sh`. Two options:

- **Option A (recommended):** Run `./demo.sh` once to bootstrap state and the
  sequencer. The demo wipes state and starts a fresh sequencer; once it
  completes the demo flow, the sequencer process is killed by the `trap`. To
  keep it alive, comment out the `trap` line in `demo.sh` and rerun — the
  sequencer process stays up after the script returns, and you can then run
  this example against it.
- **Option B:** Mirror the setup steps from `demo.sh` manually: build
  `program_methods` + `sequencer_service` + `wallet`, refresh
  `artifacts/program_methods/`, init wallet storage (`printf 'demo\n' | wallet
  config get -a`), then launch the sequencer in the background.

The example reuses `WalletCore::from_env()`, which reads `NSSA_WALLET_HOME_DIR`
to locate wallet storage and config — set it the same way `demo.sh` does:

```bash
export NSSA_WALLET_HOME_DIR="$(pwd)/wallet/configs/debug"
```

## How to run

From the repo root:

```bash
cargo run --release -p variable-supply
```

## Expected output

```
Created accounts:
  definition: <id>
  supply:     <id>
  auth_a:     <id>
  auth_b:     <id>
  holder_a:   <id>
  holder_b:   <id>

Defining fungible token 'variable-supply-demo' with auth_a as mint authority
  waiting 20s for sequencer block...

Minting 1000 to holder_a via auth_a
  waiting 20s for sequencer block...

Rotating mint authority: auth_a -> auth_b
  waiting 20s for sequencer block...

Minting 500 to holder_b via auth_b (new authority)
  waiting 20s for sequencer block...

Attempting mint via auth_a (expected to fail — auth_a no longer authorized)
  correctly rejected: Sequencer client error

Final balances:
  holder_a: 1000
  holder_b: 500

variable-supply example complete.
```

The "correctly rejected" line is the demonstration that authority rotation is
strictly enforced: the old admin can no longer mint, and the new admin can.
