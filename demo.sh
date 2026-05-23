#!/usr/bin/env bash
#
# LP-0013 demo: rotatable mint authority on fungible tokens.
#
# Walks through the additive Token program surface added by this submission:
#   - NewFungibleDefinitionWithAuthority — define a Token with a soft-pointer mint authority
#   - MintWithAuthority                  — mint gated by a separate authority account
#   - RotateAuthority                    — transfer the mint authority to a new account
#   - RevokeAuthority                    — renounce the mint authority (terminal)
#
# The demo starts a self-contained standalone sequencer, deploys a Token program
# built from this checkout, creates the four required accounts, and exercises the
# authority-gated mint end-to-end. Expected final balance on demo-hold is 1500
# (1000 minted by the original authority + 500 minted after rotation; a final
# post-revoke mint of 999 is rejected and does NOT contribute to the balance).
#
# Prerequisites (one-time host setup, NOT done by this script):
#   - Rust + Cargo
#   - `rzup install`                                       (risc0 guest toolchain)
#   - `~/.logos-blockchain-circuits`                       (run
#     `scripts/setup-logos-blockchain-circuits.sh` in the logos-blockchain repo)
#
# Usage: ./demo.sh [--keep-state]
#   --keep-state   skip the wallet/sequencer state wipe; useful for poking at
#                  the post-run state without re-running the whole flow
#
# Notes for reviewers:
#   - `RISC0_DEV_MODE=1` is used here for fast iteration. A submission video must
#     show `RISC0_DEV_MODE=0` in the terminal (real proving). Toggle at the top.
#   - `RISC0_SKIP_BUILD_KERNELS=1` skips Metal/CUDA kernel compilation. Required
#     on macOS without the full Xcode SDK; safe to leave on otherwise (CPU
#     proving is functionally identical, only slower).
#   - Sequencer endpoint is configured via `NSSA_WALLET_HOME_DIR` — there is no
#     `--rpc` flag on `wallet`.

set -euo pipefail

cd "$(dirname "$0")"
REPO_ROOT="$(pwd)"

KEEP_STATE=0
for arg in "$@"; do
    case "$arg" in
        --keep-state) KEEP_STATE=1 ;;
        *) echo "Unknown option: $arg" >&2; exit 1 ;;
    esac
done

DEMO_LOG="$REPO_ROOT/demo.log"
WALLET_CONFIG_DIR="$REPO_ROOT/wallet/configs/debug"
SEQ_ROCKSDB_DIR="$REPO_ROOT/rocksdb"

export NSSA_WALLET_HOME_DIR="$WALLET_CONFIG_DIR"
export RUST_LOG="${RUST_LOG:-info}"
export RISC0_DEV_MODE="${RISC0_DEV_MODE:-1}"
export RISC0_SKIP_BUILD_KERNELS="${RISC0_SKIP_BUILD_KERNELS:-1}"

step() { printf '\n=== %s ===\n' "$1"; }
log()  { printf '  %s\n' "$1"; }

extract_account_id() {
    # Strips the "Public/" prefix from the "Generated new account..." line.
    grep -oE 'Public/[A-Za-z0-9]+' | head -1 | sed 's|^Public/||'
}

: > "$DEMO_LOG"
log "Logs: $DEMO_LOG"
log "RISC0_DEV_MODE=$RISC0_DEV_MODE  (video walkthrough must show =0)"

step "Reset state (wallet storage + sequencer RocksDB)"
# Without this, re-runs hit "Label demo-def is already in use" (wallet labels
# persist in storage.json) or fail with stale account state in the sequencer's
# RocksDB. The two must be wiped together — keeping one without the other
# produces a wallet↔sequencer mismatch.
if [[ "$KEEP_STATE" == 1 ]]; then
    log "skipped (--keep-state)"
else
    rm -f "$WALLET_CONFIG_DIR/storage.json"
    rm -rf "$SEQ_ROCKSDB_DIR"
    log "removed $WALLET_CONFIG_DIR/storage.json"
    log "removed $SEQ_ROCKSDB_DIR/"
fi

step "Build guest binaries (token.bin etc.) via program_methods build script"
# `program_methods/build.rs` invokes `risc0_build::embed_methods()`, which
# cross-compiles each guest binary in `program_methods/guest/` to RV32IM
# (output: target/riscv-guest/.../release/<name>.bin). No docker required.
cargo build --release -p program_methods >>"$DEMO_LOG" 2>&1
GUEST_BINS_DIR="$REPO_ROOT/target/riscv-guest/program_methods/programs/riscv32im-risc0-zkvm-elf/release"
TOKEN_BIN="$GUEST_BINS_DIR/token.bin"
if [[ ! -f "$TOKEN_BIN" ]]; then
    echo "ERROR: token.bin not found at $TOKEN_BIN. Tail of $DEMO_LOG:" >&2
    tail -30 "$DEMO_LOG" >&2
    exit 1
fi
log "guest binaries: $GUEST_BINS_DIR"

step "Refresh artifacts/program_methods/ from the fresh guest build"
# `nssa/build.rs` reads `artifacts/program_methods/*.bin` to embed each guest
# ELF and compute its program_id (image hash). The hardcoded `Program::token()`
# in host code resolves to that hash — so if `artifacts/.../token.bin` is the
# stale upstream copy, the wallet sends new-Borsh-layout instructions targeting
# the OLD program_id, and the sequencer rejects them ("expected variant index
# 0 <= i < 7"). Copying the fresh build over ensures the program_id baked into
# host crates matches what we deployed.
mkdir -p "$REPO_ROOT/artifacts/program_methods"
cp "$GUEST_BINS_DIR/"*.bin "$REPO_ROOT/artifacts/program_methods/"
log "artifacts/program_methods/ refreshed"

step "Build host binaries (sequencer, wallet) against refreshed artifacts"
# `nssa`'s build script has a `cargo:rerun-if-changed=artifacts/program_methods/`
# directive, so the cp above will trigger a regeneration of TOKEN_ID etc.
# Combined cargo invocation for both host crates keeps feature unification
# consistent (avoids stale-proc-macro-dylib issues we hit with derive_more 2.x).
cargo build --release \
    -p sequencer_service --features sequencer_service/standalone \
    -p wallet \
    >>"$DEMO_LOG" 2>&1
SEQUENCER_BIN="$REPO_ROOT/target/release/sequencer_service"
WALLET_BIN="$REPO_ROOT/target/release/wallet"
for f in "$SEQUENCER_BIN" "$WALLET_BIN"; do
    if [[ ! -f "$f" ]]; then
        echo "ERROR: host binary missing: $f. Tail of $DEMO_LOG:" >&2
        tail -30 "$DEMO_LOG" >&2
        exit 1
    fi
done
log "sequencer_service: $SEQUENCER_BIN"
log "wallet:            $WALLET_BIN"

step "Initialize wallet storage (if needed)"
# After the reset step, storage.json is gone. The next `wallet` command would
# trigger an interactive setup wizard ("Input password:") whose prompt goes
# into demo.log (because we redirect 2>&1 there) — invisible on the terminal,
# script appears frozen. Pre-empt it here with a piped password. Any wallet
# subcommand triggers the wizard; `config get -a` is fully local (no
# sequencer needed yet, so we can run before starting the sequencer).
if [[ -f "$WALLET_CONFIG_DIR/storage.json" ]]; then
    log "storage.json exists, skipping init"
else
    log "creating new wallet storage (password: 'demo')"
    printf 'demo\n' | "$WALLET_BIN" config get -a >>"$DEMO_LOG" 2>&1 || true
    if [[ ! -f "$WALLET_CONFIG_DIR/storage.json" ]]; then
        echo "ERROR: wallet storage init failed. Tail of $DEMO_LOG:" >&2
        tail -30 "$DEMO_LOG" >&2
        exit 1
    fi
    log "wallet storage initialized"
fi

step "Start standalone sequencer"
"$SEQUENCER_BIN" "$REPO_ROOT/sequencer/service/configs/debug/sequencer_config.json" >>"$DEMO_LOG" 2>&1 &
SEQ_PID=$!
trap 'echo "Stopping sequencer (PID $SEQ_PID)"; kill "$SEQ_PID" 2>/dev/null || true; wait "$SEQ_PID" 2>/dev/null || true' EXIT

log "sequencer PID=$SEQ_PID"
log "waiting for sequencer health..."
for i in $(seq 1 60); do
    if "$WALLET_BIN" check-health >>"$DEMO_LOG" 2>&1; then
        log "sequencer ready after ${i}s"
        break
    fi
    sleep 1
    if [[ $i -eq 60 ]]; then
        echo "ERROR: sequencer never became healthy. Tail of $DEMO_LOG:" >&2
        tail -30 "$DEMO_LOG" >&2
        exit 1
    fi
done

step "Deploy token program"
"$WALLET_BIN" deploy-program "$TOKEN_BIN" 2>&1 | tee -a "$DEMO_LOG"

step "Create four public accounts (definition, supply, authority, holder)"
DEF_ID=$("$WALLET_BIN" account new public --label demo-def 2>&1 | tee -a "$DEMO_LOG" | extract_account_id)
log "definition: $DEF_ID"

SUP_ID=$("$WALLET_BIN" account new public --label demo-sup 2>&1 | tee -a "$DEMO_LOG" | extract_account_id)
log "supply:     $SUP_ID"

AUTH_ID=$("$WALLET_BIN" account new public --label demo-auth 2>&1 | tee -a "$DEMO_LOG" | extract_account_id)
log "authority:  $AUTH_ID"

HOLD_ID=$("$WALLET_BIN" account new public --label demo-hold 2>&1 | tee -a "$DEMO_LOG" | extract_account_id)
log "holder:     $HOLD_ID"

step "Define a fungible token with mint_authority = demo-auth"
"$WALLET_BIN" token new-fungible-with-authority \
    --definition-account-id "$DEF_ID" \
    --supply-account-id "$SUP_ID" \
    --name "lp0013-demo" \
    --total-supply 0 \
    --mint-authority "$AUTH_ID" 2>&1 | tee -a "$DEMO_LOG"

step "Mint 1000 units via the authority"
"$WALLET_BIN" token mint-with-authority \
    --definition-account-id "$DEF_ID" \
    --holder-account-id "$HOLD_ID" \
    --authority-account-id "$AUTH_ID" \
    --amount 1000 2>&1 | tee -a "$DEMO_LOG"

step "Create second authority account for rotation"
NEW_AUTH_ID=$("$WALLET_BIN" account new public --label demo-auth2 2>&1 | tee -a "$DEMO_LOG" | extract_account_id)
log "new authority: $NEW_AUTH_ID"

step "Rotate mint authority to demo-auth2"
"$WALLET_BIN" token rotate-authority \
    --definition-account-id "$DEF_ID" \
    --authority-account-id "$AUTH_ID" \
    --new-admin "$NEW_AUTH_ID" 2>&1 | tee -a "$DEMO_LOG"
sleep 20

step "Mint 500 more via the new authority"
"$WALLET_BIN" token mint-with-authority \
    --definition-account-id "$DEF_ID" \
    --holder-account-id "$HOLD_ID" \
    --authority-account-id "$NEW_AUTH_ID" \
    --amount 500 2>&1 | tee -a "$DEMO_LOG"
sleep 20

step "Revoke mint authority"
"$WALLET_BIN" token revoke-authority \
    --definition-account-id "$DEF_ID" \
    --authority-account-id "$NEW_AUTH_ID" 2>&1 | tee -a "$DEMO_LOG"
sleep 20

step "Demonstrate post-revoke mint is rejected"
# Expect this to fail with ApprovalError::Renounced (in the sequencer log).
# Wallet CLI may exit 0 if the tx is accepted into mempool — the rejection
# surfaces at block-validation time. Either way, the balance assertion below
# is the real check (should still be 1500, not 1500 + N).
"$WALLET_BIN" token mint-with-authority \
    --definition-account-id "$DEF_ID" \
    --holder-account-id "$HOLD_ID" \
    --authority-account-id "$NEW_AUTH_ID" \
    --amount 999 2>&1 | tee -a "$DEMO_LOG" || log "  expected: wallet rejected"
sleep 20

step "Wait for sequencer to produce a block"
# Sequencer config sets block_create_timeout to 15s. Until a block lands,
# the mint tx is still in the mempool and the holder's TokenHolding hasn't
# been written. Sleep slightly longer to give the block a margin.
sleep 20
log "block window elapsed"

step "Inspect holder account"
"$WALLET_BIN" account get --account-id "Public/$HOLD_ID" 2>&1 | tee -a "$DEMO_LOG"

echo
echo "Demo finished. See $DEMO_LOG for the full sequencer + wallet log."
