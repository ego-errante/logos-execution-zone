//! LP-0013 example: fixed-supply token with revoked mint authority.
//!
//! Demonstrates the canonical "stablecoin / capped-supply asset" pattern enabled by
//! `lez-approval` + the Token program's `RevokeAuthority` instruction:
//!
//!   1. Define a fungible token whose initial supply is minted up-front to a
//!      named authority.
//!   2. Mint the full pre-allocated supply to a holder via that authority.
//!   3. Terminally revoke the authority — supply is now permanently fixed.
//!   4. Attempt one more mint and observe the sequencer reject it (the guest
//!      panics with `ApprovalError::Renounced`, which surfaces as a
//!      `SequencerClientError` on the wallet facade).
//!   5. Print the holder's final balance (1000).
//!
//! Prerequisites (NOT done by this example):
//!   - A standalone sequencer running at `127.0.0.1:3040` with the token program
//!     deployed. The simplest way is to run `./demo.sh --keep-state` once and
//!     then leave the sequencer process running (the trap in demo.sh kills it
//!     on exit — either remove the trap or start the sequencer separately).
//!   - The wallet has storage initialized at `$NSSA_WALLET_HOME_DIR` (defaults
//!     to `wallet/configs/debug` when invoked via `demo.sh`).
//!
//! Run with:
//!   cargo run --release -p fixed-supply

use std::time::Duration;

use anyhow::{Context as _, Result};
use nssa_core::account::AccountId;
use sequencer_service_rpc::RpcClient as _;
use token_core::TokenHolding;
use wallet::{WalletCore, program_facades::token::Token};

/// Matches the sequencer's `block_create_timeout` in the standalone config
/// (currently 15s) with a small margin so the freshly-submitted tx has landed
/// in a block before we read state.
const BLOCK_WAIT: Duration = Duration::from_secs(20);

#[tokio::main]
async fn main() -> Result<()> {
    let mut wallet_core =
        WalletCore::from_env().context("failed to initialize WalletCore from env")?;

    // --- 1. Create the four accounts: definition, supply, authority, holder.
    let (def_id, _) = wallet_core.create_new_account_public(None);
    let (sup_id, _) = wallet_core.create_new_account_public(None);
    let (auth_id, _) = wallet_core.create_new_account_public(None);
    let (hold_id, _) = wallet_core.create_new_account_public(None);

    println!("Created accounts:");
    println!("  definition: {def_id}");
    println!("  supply:     {sup_id}");
    println!("  authority:  {auth_id}");
    println!("  holder:     {hold_id}");

    // --- 2. Define a fungible token with mint authority.
    //
    // total_supply is 0 here because we want all supply to come from the
    // post-definition mint (which is gated by the authority). Some integrations
    // prefer the alternative shape — pre-mint the cap into the supply account
    // and renounce immediately — but the "mint then revoke" shape exercises the
    // full authority lifecycle end-to-end, which is what this example showcases.
    println!("\nDefining fungible token 'fixed-supply-demo' with mint authority");
    Token(&wallet_core)
        .send_new_definition_with_authority(
            def_id,
            sup_id,
            "fixed-supply-demo".to_owned(),
            0,
            Some(auth_id),
        )
        .await
        .context("send_new_definition_with_authority failed")?;
    println!("  waiting {BLOCK_WAIT:?} for sequencer block...");
    tokio::time::sleep(BLOCK_WAIT).await;

    // --- 3. Mint the fixed supply (1000) to the holder via the authority.
    println!("\nMinting 1000 units to holder via authority");
    Token(&wallet_core)
        .send_mint_with_authority(def_id, hold_id, auth_id, 1000)
        .await
        .context("initial mint via authority failed")?;
    println!("  waiting {BLOCK_WAIT:?} for sequencer block...");
    tokio::time::sleep(BLOCK_WAIT).await;

    // --- 4. Revoke the authority. Supply is now permanently fixed at 1000.
    println!("\nRevoking mint authority (terminal — supply is now fixed)");
    Token(&wallet_core)
        .send_revoke_authority(def_id, auth_id)
        .await
        .context("revoke_authority failed")?;
    println!("  waiting {BLOCK_WAIT:?} for sequencer block...");
    tokio::time::sleep(BLOCK_WAIT).await;

    // --- 5. Attempt another mint — must fail with ApprovalError::Renounced.
    //
    // The wallet facade's send_mint_with_authority returns
    // Result<HashType, ExecutionFailureKind>. When the guest panics
    // (`Authority.gate(...)` panics with `ApprovalError::Renounced`'s Display),
    // the sequencer rejects the tx and the JSON-RPC error surfaces as
    // ExecutionFailureKind::SequencerClientError. So no catch_unwind is needed
    // — the rejection is a plain Err.
    println!("\nAttempting post-revoke mint (expected to fail with ApprovalError::Renounced)");
    match Token(&wallet_core)
        .send_mint_with_authority(def_id, hold_id, auth_id, 1)
        .await
    {
        Ok(tx_hash) => {
            anyhow::bail!(
                "post-revoke mint unexpectedly succeeded (tx_hash={tx_hash:?}); \
                 authority should have been renounced"
            );
        }
        Err(err) => {
            println!("  correctly rejected: {err}");
        }
    }

    // --- 6. Read and print the holder's final balance.
    let final_balance = read_fungible_balance(&wallet_core, hold_id).await?;
    println!("\nFinal holder balance: {final_balance}");
    assert_eq!(
        final_balance, 1000,
        "expected exactly the pre-revoke mint (1000) to have landed"
    );

    println!("\nfixed-supply example complete.");
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
