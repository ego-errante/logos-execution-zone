use nssa_core::{
    account::{Account, AccountWithMetadata, Data},
    program::{AccountPostState, Claim},
};
use token_core::{TokenDefinition, TokenHolding};

#[must_use]
pub fn mint(
    definition_account: AccountWithMetadata,
    user_holding_account: AccountWithMetadata,
    amount_to_mint: u128,
) -> Vec<AccountPostState> {
    assert!(
        definition_account.is_authorized,
        "Definition authorization is missing"
    );

    let mut definition = TokenDefinition::try_from(&definition_account.account.data)
        .expect("Token Definition account must be valid");
    let mut holding = if user_holding_account.account == Account::default() {
        TokenHolding::zeroized_from_definition(definition_account.account_id, &definition)
    } else {
        TokenHolding::try_from(&user_holding_account.account.data)
            .expect("Token Holding account must be valid")
    };

    assert_eq!(
        definition_account.account_id,
        holding.definition_id(),
        "Mismatch Token Definition and Token Holding"
    );

    match (&mut definition, &mut holding) {
        (
            TokenDefinition::Fungible {
                name: _,
                metadata_id: _,
                authority: _,
                total_supply,
            },
            TokenHolding::Fungible {
                definition_id: _,
                balance,
            },
        ) => {
            *balance = balance
                .checked_add(amount_to_mint)
                .expect("Balance overflow on minting");

            *total_supply = total_supply
                .checked_add(amount_to_mint)
                .expect("Total supply overflow");
        }
        (
            TokenDefinition::NonFungible { .. },
            TokenHolding::NftMaster { .. } | TokenHolding::NftPrintedCopy { .. },
        ) => {
            panic!("Cannot mint additional supply for Non-Fungible Tokens");
        }
        _ => panic!("Mismatched Token Definition and Token Holding types"),
    }

    let mut definition_post = definition_account.account;
    definition_post.data = Data::from(&definition);

    let mut holding_post = user_holding_account.account;
    holding_post.data = Data::from(&holding);

    vec![
        AccountPostState::new(definition_post),
        AccountPostState::new_claimed_if_default(holding_post, Claim::Authorized),
    ]
}

/// Authority-gated mint (LP-0013 / RFP-001).
///
/// Verifies the `authority_account`'s id matches the active admin in the
/// [`Authority`](token_core::Authority) recorded on the Token Definition and
/// that the authority account carries an authorization claim — i.e. its keypair
/// signed the enclosing transaction. The Token Definition account itself does
/// **not** need to be authorized: the authority is the signer.
///
/// Panics with [`ApprovalError::Renounced`](token_core::ApprovalError::Renounced)
/// if the definition's authority has been renounced, or with
/// [`ApprovalError::Unauthorized`](token_core::ApprovalError::Unauthorized) if
/// the supplied authority account does not match or is not authorized. The panic
/// message is the [`Display`] form of the variant and is stable across versions.
#[must_use]
pub fn mint_with_authority(
    definition_account: AccountWithMetadata,
    user_holding_account: AccountWithMetadata,
    authority_account: AccountWithMetadata,
    amount_to_mint: u128,
) -> Vec<AccountPostState> {
    let mut definition = TokenDefinition::try_from(&definition_account.account.data)
        .expect("Token Definition account must be valid");

    let authority = match &definition {
        TokenDefinition::Fungible { authority, .. } => *authority,
        TokenDefinition::NonFungible { .. } => {
            panic!("mint_with_authority is only supported for fungible tokens");
        }
    };

    authority.gate(&authority_account);

    let mut holding = if user_holding_account.account == Account::default() {
        TokenHolding::zeroized_from_definition(definition_account.account_id, &definition)
    } else {
        TokenHolding::try_from(&user_holding_account.account.data)
            .expect("Token Holding account must be valid")
    };

    assert_eq!(
        definition_account.account_id,
        holding.definition_id(),
        "Mismatch Token Definition and Token Holding"
    );

    match (&mut definition, &mut holding) {
        (
            TokenDefinition::Fungible {
                name: _,
                metadata_id: _,
                authority: _,
                total_supply,
            },
            TokenHolding::Fungible {
                definition_id: _,
                balance,
            },
        ) => {
            *balance = balance
                .checked_add(amount_to_mint)
                .expect("Balance overflow on minting");

            *total_supply = total_supply
                .checked_add(amount_to_mint)
                .expect("Total supply overflow");
        }
        _ => panic!("Mismatched Token Definition and Token Holding types"),
    }

    let mut definition_post = definition_account.account;
    definition_post.data = Data::from(&definition);

    let mut holding_post = user_holding_account.account;
    holding_post.data = Data::from(&holding);

    let authority_post = authority_account.account;

    vec![
        AccountPostState::new(definition_post),
        AccountPostState::new_claimed_if_default(holding_post, Claim::Authorized),
        AccountPostState::new(authority_post),
    ]
}
