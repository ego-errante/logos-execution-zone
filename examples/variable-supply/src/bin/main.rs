//! LP-0013 example: variable-supply token with rotating mint authority.
//!
//! Demonstrates the canonical "ongoing issuance with hand-off" pattern enabled by
//! `lez-approval` + the Token program's `RotateAuthority` instruction:
//!
//!   1. Define a fungible token with an initial mint authority (`auth_a`).
//!   2. `auth_a` mints 1000 to `holder_a`.
//!   3. Rotate the authority from `auth_a` to `auth_b` (e.g. multisig handoff,
//!      DAO governance transition, key rotation).
//!   4. `auth_b` mints 500 to `holder_b`.
//!   5. Attempt a mint via `auth_a` again — must fail (the gate now checks
//!      against `auth_b`'s id).
//!   6. Print both holders' final balances (1000 and 500).
//!
//! Prerequisites (NOT done by this example):
//!   - A standalone sequencer running at `127.0.0.1:3040` with the token program
//!     deployed. The simplest way is to run `./demo.sh --keep-state` once and
//!     then leave the sequencer process running (the trap in demo.sh kills it
//!     on exit — either remove the trap or start the sequencer separately).
//!   - The wallet has storage initialized at `$NSSA_WALLET_HOME_DIR`.
//!
//! Run with:
//!   cargo run --release -p variable-supply

use std::time::Duration;

use anyhow::{Context as _, Result};
use nssa_core::account::AccountId;
use sequencer_service_rpc::RpcClient as _;
use token_core::TokenHolding;
use wallet::{WalletCore, program_facades::token::Token};

/// Matches the sequencer's `block_create_timeout` in the standalone config
/// (currently 15s) with a small margin so the freshly-submitted tx has landed
/// in a block before we read state or submit the next dependent tx.
const BLOCK_WAIT: Duration = Duration::from_secs(20);

#[tokio::main]
async fn main() -> Result<()> {
    let mut wallet_core =
        WalletCore::from_env().context("failed to initialize WalletCore from env")?;

    // --- 1. Create the six accounts: definition, supply, two authorities, two holders.
    let (def_id, _) = wallet_core.create_new_account_public(None);
    let (sup_id, _) = wallet_core.create_new_account_public(None);
    let (auth_a_id, _) = wallet_core.create_new_account_public(None);
    let (auth_b_id, _) = wallet_core.create_new_account_public(None);
    let (holder_a_id, _) = wallet_core.create_new_account_public(None);
    let (holder_b_id, _) = wallet_core.create_new_account_public(None);

    println!("Created accounts:");
    println!("  definition: {def_id}");
    println!("  supply:     {sup_id}");
    println!("  auth_a:     {auth_a_id}");
    println!("  auth_b:     {auth_b_id}");
    println!("  holder_a:   {holder_a_id}");
    println!("  holder_b:   {holder_b_id}");

    // --- 2. Define a fungible token with auth_a as the initial mint authority.
    //
    // total_supply is 0 (no pre-allocation); all supply comes from authority-gated
    // mints over time, which is the defining property of a variable-supply token.
    println!("\nDefining fungible token 'variable-supply-demo' with auth_a as mint authority");
    Token(&wallet_core)
        .send_new_definition_with_authority(
            def_id,
            sup_id,
            "variable-supply-demo".to_owned(),
            0,
            Some(auth_a_id),
        )
        .await
        .context("send_new_definition_with_authority failed")?;
    println!("  waiting {BLOCK_WAIT:?} for sequencer block...");
    tokio::time::sleep(BLOCK_WAIT).await;

    // --- 3. auth_a mints 1000 to holder_a.
    println!("\nMinting 1000 to holder_a via auth_a");
    Token(&wallet_core)
        .send_mint_with_authority(def_id, holder_a_id, auth_a_id, 1000)
        .await
        .context("first mint via auth_a failed")?;
    println!("  waiting {BLOCK_WAIT:?} for sequencer block...");
    tokio::time::sleep(BLOCK_WAIT).await;

    // --- 4. Rotate the authority from auth_a to auth_b.
    println!("\nRotating mint authority: auth_a -> auth_b");
    Token(&wallet_core)
        .send_rotate_authority(def_id, auth_a_id, auth_b_id)
        .await
        .context("rotate_authority failed")?;
    println!("  waiting {BLOCK_WAIT:?} for sequencer block...");
    tokio::time::sleep(BLOCK_WAIT).await;

    // --- 5. auth_b mints 500 to holder_b.
    println!("\nMinting 500 to holder_b via auth_b (new authority)");
    Token(&wallet_core)
        .send_mint_with_authority(def_id, holder_b_id, auth_b_id, 500)
        .await
        .context("post-rotate mint via auth_b failed")?;
    println!("  waiting {BLOCK_WAIT:?} for sequencer block...");
    tokio::time::sleep(BLOCK_WAIT).await;

    // --- 6. Attempt mint via auth_a — must fail (old authority is no longer gated in).
    //
    // The wallet facade returns Result<HashType, ExecutionFailureKind>. When the
    // guest panics with ApprovalError::Unauthorized, the sequencer rejects the tx
    // and the JSON-RPC error surfaces as ExecutionFailureKind::SequencerClientError.
    // So no catch_unwind is needed — the rejection is a plain Err.
    println!("\nAttempting mint via auth_a (expected to fail — auth_a no longer authorized)");
    match Token(&wallet_core)
        .send_mint_with_authority(def_id, holder_a_id, auth_a_id, 1)
        .await
    {
        Ok(tx_hash) => {
            anyhow::bail!(
                "post-rotation mint via auth_a unexpectedly succeeded (tx_hash={tx_hash:?}); \
                 auth_a should no longer be the active authority"
            );
        }
        Err(err) => {
            println!("  correctly rejected: {err}");
        }
    }

    // --- 7. Read and print both holders' final balances.
    let bal_a = read_fungible_balance(&wallet_core, holder_a_id).await?;
    let bal_b = read_fungible_balance(&wallet_core, holder_b_id).await?;
    println!("\nFinal balances:");
    println!("  holder_a: {bal_a}");
    println!("  holder_b: {bal_b}");
    assert_eq!(bal_a, 1000, "auth_a's pre-rotation mint should have landed");
    assert_eq!(
        bal_b, 500,
        "auth_b's post-rotation mint should have landed"
    );

    println!("\nvariable-supply example complete.");
    Ok(())
}

/// Pull the account from the sequencer, decode its data as a `TokenHolding`,
/// and return the fungible balance. Errors if the account isn't a Fungible
/// holding (e.g. uninitialized or NFT).
async fn read_fungible_balance(wallet_core: &WalletCore, holder_id: AccountId) -> Result<u128> {
    let acc = wallet_core
        .sequencer_client
        .get_account(holder_id)
        .await
        .context("sequencer get_account failed")?;
    let holding =
        TokenHolding::try_from(&acc.data).context("holder account data is not a TokenHolding")?;
    match holding {
        TokenHolding::Fungible { balance, .. } => Ok(balance),
        other => anyhow::bail!("expected Fungible holding, got {other:?}"),
    }
}
