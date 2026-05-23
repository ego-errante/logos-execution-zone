//! Integration tests for the LP-0013 mint-authority lifecycle:
//! `NewFungibleDefinitionWithAuthority` + `MintWithAuthority` + `RotateAuthority`
//! + `RevokeAuthority`, exercised end-to-end through the wallet facade against a
//! sequencer spun up by [`TestContext`].
//!
//! The wallet facade submits transactions and returns a tx hash on successful
//! submission; on-chain rejection (e.g. an `ApprovalError::Unauthorized` /
//! `ApprovalError::Renounced` panic in the guest) happens later during block
//! execution and is NOT surfaced back through the facade's `Result`. Failure
//! tests therefore submit the would-be-failing tx, wait for the next block, and
//! assert that the persisted state (holder balance, definition authority) is
//! unchanged.

#![expect(
    clippy::shadow_unrelated,
    clippy::tests_outside_test_module,
    reason = "We don't care about these in tests"
)]

use std::time::Duration;

use anyhow::Result;
use integration_tests::{TIME_TO_WAIT_FOR_BLOCK_SECONDS, TestContext};
use log::info;
use nssa::AccountId;
use sequencer_service_rpc::RpcClient as _;
use token_core::{Authority, TokenDefinition, TokenHolding};
use tokio::test;
use wallet::{
    cli::{
        Command, SubcommandReturnValue,
        account::{AccountSubcommand, NewSubcommand},
    },
    program_facades::token::Token,
};

/// Create a new public account in `ctx`'s wallet and return its id.
async fn new_public_account(ctx: &mut TestContext) -> Result<AccountId> {
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: None,
            label: None,
        })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount { account_id } = result else {
        anyhow::bail!("Expected RegisterAccount return value");
    };
    Ok(account_id)
}

/// Read the on-chain `TokenDefinition` for `definition_id`.
async fn read_definition(ctx: &TestContext, definition_id: AccountId) -> Result<TokenDefinition> {
    let acc = ctx.sequencer_client().get_account(definition_id).await?;
    Ok(TokenDefinition::try_from(&acc.data)?)
}

/// Read the on-chain fungible balance held by `holder_id` (returns 0 if the
/// account isn't yet initialised as a holding — i.e. the failure case where
/// the mint never went through).
async fn read_balance(ctx: &TestContext, holder_id: AccountId) -> Result<u128> {
    let acc = ctx.sequencer_client().get_account(holder_id).await?;
    if acc.data.as_ref().is_empty() {
        return Ok(0);
    }
    let holding = TokenHolding::try_from(&acc.data)?;
    let TokenHolding::Fungible { balance, .. } = holding else {
        anyhow::bail!("expected fungible holding, got {:?}", holding);
    };
    Ok(balance)
}

async fn wait_for_block() {
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;
}

/// Define-with-authority + mint via that authority succeeds, holder balance
/// equals minted amount.
#[test]
async fn mint_with_authority_happy_path() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let definition_id = new_public_account(&mut ctx).await?;
    let supply_id = new_public_account(&mut ctx).await?;
    let authority_id = new_public_account(&mut ctx).await?;
    let holder_id = new_public_account(&mut ctx).await?;

    let name = "AUTH TOKEN".to_owned();
    let total_supply: u128 = 0;
    let mint_amount: u128 = 1000;

    Token(ctx.wallet())
        .send_new_definition_with_authority(
            definition_id,
            supply_id,
            name.clone(),
            total_supply,
            Some(authority_id),
        )
        .await?;
    wait_for_block().await;

    // Sanity-check the authority landed on the definition.
    let def = read_definition(&ctx, definition_id).await?;
    assert_eq!(
        def,
        TokenDefinition::Fungible {
            name: name.clone(),
            total_supply,
            metadata_id: None,
            authority: Authority::new(authority_id),
        }
    );

    Token(ctx.wallet())
        .send_mint_with_authority(definition_id, holder_id, authority_id, mint_amount)
        .await?;
    wait_for_block().await;

    assert_eq!(read_balance(&ctx, holder_id).await?, mint_amount);

    info!("mint_with_authority_happy_path succeeded");
    Ok(())
}

/// After rotation, the new admin can mint successfully and the on-chain
/// authority field reflects the new admin.
#[test]
async fn rotate_authority_then_mint_with_new_admin() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let definition_id = new_public_account(&mut ctx).await?;
    let supply_id = new_public_account(&mut ctx).await?;
    let old_authority_id = new_public_account(&mut ctx).await?;
    let new_authority_id = new_public_account(&mut ctx).await?;
    let holder_id = new_public_account(&mut ctx).await?;

    let name = "ROTATE TOKEN".to_owned();
    let total_supply: u128 = 0;
    let mint_amount: u128 = 500;

    Token(ctx.wallet())
        .send_new_definition_with_authority(
            definition_id,
            supply_id,
            name.clone(),
            total_supply,
            Some(old_authority_id),
        )
        .await?;
    wait_for_block().await;

    Token(ctx.wallet())
        .send_rotate_authority(definition_id, old_authority_id, new_authority_id)
        .await?;
    wait_for_block().await;

    // Authority field now points at new admin.
    let def = read_definition(&ctx, definition_id).await?;
    assert_eq!(
        def,
        TokenDefinition::Fungible {
            name: name.clone(),
            total_supply,
            metadata_id: None,
            authority: Authority::new(new_authority_id),
        }
    );

    // New admin can mint.
    Token(ctx.wallet())
        .send_mint_with_authority(definition_id, holder_id, new_authority_id, mint_amount)
        .await?;
    wait_for_block().await;

    assert_eq!(read_balance(&ctx, holder_id).await?, mint_amount);

    info!("rotate_authority_then_mint_with_new_admin succeeded");
    Ok(())
}

/// After rotation, a mint attempt signed by the OLD authority must NOT update
/// the holder's balance — the guest panics with `ApprovalError::Unauthorized`,
/// the transaction is rejected during block execution, and the holding account
/// is never created.
#[test]
async fn rotate_authority_blocks_old_admin() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let definition_id = new_public_account(&mut ctx).await?;
    let supply_id = new_public_account(&mut ctx).await?;
    let old_authority_id = new_public_account(&mut ctx).await?;
    let new_authority_id = new_public_account(&mut ctx).await?;
    let holder_id = new_public_account(&mut ctx).await?;

    let name = "ROTATE BLOCKS OLD".to_owned();
    let total_supply: u128 = 0;
    let mint_amount: u128 = 777;

    Token(ctx.wallet())
        .send_new_definition_with_authority(
            definition_id,
            supply_id,
            name.clone(),
            total_supply,
            Some(old_authority_id),
        )
        .await?;
    wait_for_block().await;

    Token(ctx.wallet())
        .send_rotate_authority(definition_id, old_authority_id, new_authority_id)
        .await?;
    wait_for_block().await;

    // Attempt mint with the now-stale OLD authority. Submission may succeed
    // (the sequencer accepts the tx for inclusion); the guest panics on
    // execution. Either way the holder balance must stay 0.
    let _ = Token(ctx.wallet())
        .send_mint_with_authority(definition_id, holder_id, old_authority_id, mint_amount)
        .await;
    wait_for_block().await;

    assert_eq!(
        read_balance(&ctx, holder_id).await?,
        0,
        "old admin's mint must NOT have updated the holder balance",
    );

    // Definition's authority field unchanged (still new_authority_id).
    let def = read_definition(&ctx, definition_id).await?;
    assert_eq!(
        def,
        TokenDefinition::Fungible {
            name,
            total_supply,
            metadata_id: None,
            authority: Authority::new(new_authority_id),
        }
    );

    info!("rotate_authority_blocks_old_admin succeeded");
    Ok(())
}

/// After revocation, every subsequent `MintWithAuthority` is rejected
/// (`ApprovalError::Renounced`) regardless of signer. Holder balance never
/// moves and authority field stays `renounced`.
#[test]
async fn revoke_authority_blocks_all_subsequent_mints() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let definition_id = new_public_account(&mut ctx).await?;
    let supply_id = new_public_account(&mut ctx).await?;
    let authority_id = new_public_account(&mut ctx).await?;
    let holder_id = new_public_account(&mut ctx).await?;

    let name = "REVOKE TOKEN".to_owned();
    let total_supply: u128 = 0;
    let mint_amount: u128 = 250;

    Token(ctx.wallet())
        .send_new_definition_with_authority(
            definition_id,
            supply_id,
            name.clone(),
            total_supply,
            Some(authority_id),
        )
        .await?;
    wait_for_block().await;

    // Revoke first — signed by the (then-current) authority.
    Token(ctx.wallet())
        .send_revoke_authority(definition_id, authority_id)
        .await?;
    wait_for_block().await;

    let def = read_definition(&ctx, definition_id).await?;
    assert_eq!(
        def,
        TokenDefinition::Fungible {
            name: name.clone(),
            total_supply,
            metadata_id: None,
            authority: Authority::renounced(),
        }
    );

    // Now attempt to mint. Authority gate is renounced — guest must panic.
    let _ = Token(ctx.wallet())
        .send_mint_with_authority(definition_id, holder_id, authority_id, mint_amount)
        .await;
    wait_for_block().await;

    assert_eq!(
        read_balance(&ctx, holder_id).await?,
        0,
        "post-revoke mint must not update holder balance",
    );

    // Authority still renounced.
    let def = read_definition(&ctx, definition_id).await?;
    assert_eq!(
        def,
        TokenDefinition::Fungible {
            name,
            total_supply,
            metadata_id: None,
            authority: Authority::renounced(),
        }
    );

    info!("revoke_authority_blocks_all_subsequent_mints succeeded");
    Ok(())
}

/// Revoke must be performed by the CURRENT authority (i.e. after a rotation,
/// the new admin can revoke). Confirms (a) the new admin's revoke succeeds —
/// authority flips to renounced — and (b) the old admin's subsequent mint
/// attempt is still rejected, so the holder balance never moves.
#[test]
async fn revoke_after_rotate_uses_current_authority() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let definition_id = new_public_account(&mut ctx).await?;
    let supply_id = new_public_account(&mut ctx).await?;
    let old_authority_id = new_public_account(&mut ctx).await?;
    let new_authority_id = new_public_account(&mut ctx).await?;
    let holder_id = new_public_account(&mut ctx).await?;

    let name = "REVOKE AFTER ROTATE".to_owned();
    let total_supply: u128 = 0;
    let mint_amount: u128 = 111;

    Token(ctx.wallet())
        .send_new_definition_with_authority(
            definition_id,
            supply_id,
            name.clone(),
            total_supply,
            Some(old_authority_id),
        )
        .await?;
    wait_for_block().await;

    Token(ctx.wallet())
        .send_rotate_authority(definition_id, old_authority_id, new_authority_id)
        .await?;
    wait_for_block().await;

    // New admin revokes — must succeed.
    Token(ctx.wallet())
        .send_revoke_authority(definition_id, new_authority_id)
        .await?;
    wait_for_block().await;

    let def = read_definition(&ctx, definition_id).await?;
    assert_eq!(
        def,
        TokenDefinition::Fungible {
            name: name.clone(),
            total_supply,
            metadata_id: None,
            authority: Authority::renounced(),
        },
        "new admin's revoke should have flipped authority to renounced",
    );

    // Old admin's mint attempt — already deposed by the rotation, now also
    // blocked by the revocation. Either gate is enough; holder balance must
    // not move.
    let _ = Token(ctx.wallet())
        .send_mint_with_authority(definition_id, holder_id, old_authority_id, mint_amount)
        .await;
    wait_for_block().await;

    assert_eq!(read_balance(&ctx, holder_id).await?, 0);

    info!("revoke_after_rotate_uses_current_authority succeeded");
    Ok(())
}
