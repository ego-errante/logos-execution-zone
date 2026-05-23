# fixed-supply

LP-0013 example integration: a **fungible token with a capped, permanently-fixed supply**.

This is the canonical "issue then renounce" pattern, suitable for stablecoins,
airdropped utility tokens, or any asset where the issuer commits up-front to a
hard supply cap that no future minter can violate.

## What it demonstrates

The example runs a complete fixed-supply lifecycle against a standalone sequencer:

1. Creates four public accounts (definition, supply, authority, holder).
2. Calls `NewFungibleDefinitionWithAuthority` to define a token named
   `fixed-supply-demo` with the authority as its mint admin.
3. Calls `MintWithAuthority` to mint **1000** units to the holder.
4. Calls `RevokeAuthority` — the supply is now permanently frozen at 1000.
5. Attempts one more `MintWithAuthority` and observes the sequencer reject it
   with `ApprovalError::Renounced` (this is the RFP-001 guarantee in action).
6. Reads and asserts the holder's final balance is exactly 1000.

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
cargo run --release -p fixed-supply
```

## Expected output

```
Created accounts:
  definition: <id>
  supply:     <id>
  authority:  <id>
  holder:     <id>

Defining fungible token 'fixed-supply-demo' with mint authority
  waiting 20s for sequencer block...

Minting 1000 units to holder via authority
  waiting 20s for sequencer block...

Revoking mint authority (terminal — supply is now fixed)
  waiting 20s for sequencer block...

Attempting post-revoke mint (expected to fail with ApprovalError::Renounced)
  correctly rejected: Sequencer client error

Final holder balance: 1000

fixed-supply example complete.
```

The "correctly rejected" line is the demonstration of LP-0013's deterministic
revoked-mint rejection requirement: the guest panicked with
`ApprovalError::Renounced` and the sequencer surfaced it back through the RPC.
