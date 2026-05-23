//! Authority rotation and revocation handlers (LP-0013 / RFP-001).
//!
//! Both handlers gate on the current admin recorded in
//! [`TokenDefinition::Fungible::authority`] before mutating the definition.
//! Account ordering: `[definition, current_authority]`. The new admin (for
//! rotation) is carried in the instruction args rather than as a third
//! account, mirroring how `NewFungibleDefinitionWithAuthority` takes its
//! initial `mint_authority` as an arg.

use nssa_core::{
    account::{AccountWithMetadata, Data, AccountId},
    program::AccountPostState,
};
use token_core::TokenDefinition;

/// Rotate the recorded authority on a Token Definition to `new_admin`.
///
/// Panics with [`ApprovalError::Renounced`](token_core::ApprovalError::Renounced)
/// if the definition's authority has already been revoked, or with
/// [`ApprovalError::Unauthorized`](token_core::ApprovalError::Unauthorized) if
/// the supplied authority account is not authorized or does not match the
/// current admin recorded on the definition.
#[must_use]
pub fn rotate_authority(
    definition_account: AccountWithMetadata,
    authority_account: AccountWithMetadata,
    new_admin: AccountId,
) -> Vec<AccountPostState> {
    let mut definition = TokenDefinition::try_from(&definition_account.account.data)
        .expect("Token Definition account must be valid");

    let TokenDefinition::Fungible { authority, .. } = &mut definition else {
        panic!("rotate_authority is only supported for fungible tokens");
    };
    authority.transfer(&authority_account, new_admin);

    let mut definition_post = definition_account.account;
    definition_post.data = Data::from(&definition);
    let authority_post = authority_account.account;

    vec![
        AccountPostState::new(definition_post),
        AccountPostState::new(authority_post),
    ]
}

/// Terminally renounce the recorded authority on a Token Definition.
///
/// Panics with [`ApprovalError::Renounced`](token_core::ApprovalError::Renounced)
/// if the definition's authority has already been revoked, or with
/// [`ApprovalError::Unauthorized`](token_core::ApprovalError::Unauthorized) if
/// the supplied authority account is not authorized or does not match the
/// current admin recorded on the definition.
#[must_use]
pub fn revoke_authority(
    definition_account: AccountWithMetadata,
    authority_account: AccountWithMetadata,
) -> Vec<AccountPostState> {
    let mut definition = TokenDefinition::try_from(&definition_account.account.data)
        .expect("Token Definition account must be valid");

    let TokenDefinition::Fungible { authority, .. } = &mut definition else {
        panic!("revoke_authority is only supported for fungible tokens");
    };
    authority.revoke(&authority_account);

    let mut definition_post = definition_account.account;
    definition_post.data = Data::from(&definition);
    let authority_post = authority_account.account;

    vec![
        AccountPostState::new(definition_post),
        AccountPostState::new(authority_post),
    ]
}
