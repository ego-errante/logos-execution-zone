//! SPEL-shape mirror of the LP-0013 Token program.
//!
//! This file is the source of provenance for `artifacts/token.idl.spel.json`,
//! emitted by `spel generate-idl` (see `spel-sidecar/README.md` for the exact
//! invocation and `docs/SPEL_STATUS.md` for why a sidecar exists).
//!
//! Handler bodies are intentionally minimal — `spel generate-idl` only parses
//! the function signatures + `#[account(...)]` attributes; it never compiles
//! or runs this code. The mirror keeps the instruction names, argument shapes,
//! and account ordering aligned with `programs/token/core/src/lib.rs`
//! (`Instruction` enum) and the dispatcher in
//! `program_methods/guest/src/bin/token.rs`. The canonical, byte-shape-complete
//! IDL is `artifacts/token.idl.json` (hand-authored against `SpelIdl`).

#![allow(dead_code, unused_imports, unused_variables)]

use spel_framework::prelude::*;

#[lez_program]
mod token {
    #[allow(unused_imports)]
    use super::*;

    /// Transfer tokens from sender to recipient.
    #[instruction]
    pub fn transfer(
        #[account(mut, signer)] sender_holding: AccountWithMetadata,
        #[account(mut)] recipient_holding: AccountWithMetadata,
        amount_to_transfer: u128,
    ) -> SpelResult {
        Ok(SpelOutput::execute(
            vec![sender_holding, recipient_holding],
            vec![],
        ))
    }

    /// Create a new fungible token definition without metadata.
    #[instruction]
    pub fn new_fungible_definition(
        #[account(init, signer)] definition: AccountWithMetadata,
        #[account(init, signer)] holding: AccountWithMetadata,
        name: String,
        total_supply: u128,
    ) -> SpelResult {
        Ok(SpelOutput::execute(vec![definition, holding], vec![]))
    }

    /// Create a new fungible definition with a rotatable mint authority
    /// (LP-0013 / RFP-001). `mint_authority` carries the AccountId bytes of
    /// the initial admin; `[0u8; 32]` is treated as the renounced sentinel by
    /// the canonical handler (see `artifacts/token.idl.json` for the wire
    /// shape with `Option<AccountId>`).
    #[instruction]
    pub fn new_fungible_definition_with_authority(
        #[account(init, signer)] definition: AccountWithMetadata,
        #[account(init, signer)] holding: AccountWithMetadata,
        name: String,
        total_supply: u128,
        mint_authority: [u8; 32],
    ) -> SpelResult {
        Ok(SpelOutput::execute(vec![definition, holding], vec![]))
    }

    /// Create a new fungible or non-fungible definition with metadata.
    #[instruction]
    pub fn new_definition_with_metadata(
        #[account(init, signer)] definition: AccountWithMetadata,
        #[account(init, signer)] holding: AccountWithMetadata,
        #[account(init, signer)] metadata: AccountWithMetadata,
        new_definition: NewTokenDefinition,
        metadata_info: NewTokenMetadata,
    ) -> SpelResult {
        Ok(SpelOutput::execute(
            vec![definition, holding, metadata],
            vec![],
        ))
    }

    /// Initialize a token holding account for an existing token definition.
    #[instruction]
    pub fn initialize_account(
        #[account()] definition: AccountWithMetadata,
        #[account(init, signer)] holding: AccountWithMetadata,
    ) -> SpelResult {
        Ok(SpelOutput::execute(vec![definition, holding], vec![]))
    }

    /// Burn tokens from the holder's account.
    #[instruction]
    pub fn burn(
        #[account()] definition: AccountWithMetadata,
        #[account(mut, signer)] holding: AccountWithMetadata,
        amount_to_burn: u128,
    ) -> SpelResult {
        Ok(SpelOutput::execute(vec![definition, holding], vec![]))
    }

    /// Mint new tokens via the legacy (definition-signs) path. Not gated by
    /// any recorded authority on the definition.
    #[instruction]
    pub fn mint(
        #[account(mut, signer)] definition: AccountWithMetadata,
        #[account(mut)] holding: AccountWithMetadata,
        amount_to_mint: u128,
    ) -> SpelResult {
        Ok(SpelOutput::execute(vec![definition, holding], vec![]))
    }

    /// Mint new tokens, gated by the rotatable authority recorded on the
    /// Token Definition (LP-0013 / RFP-001). The third account is the active
    /// admin; the handler panics with `Unauthorized` if its id does not equal
    /// the recorded authority, or `Renounced` if the authority is revoked.
    #[instruction]
    pub fn mint_with_authority(
        #[account()] definition: AccountWithMetadata,
        #[account(mut)] holding: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        amount_to_mint: u128,
    ) -> SpelResult {
        Ok(SpelOutput::execute(
            vec![definition, holding, authority],
            vec![],
        ))
    }

    /// Print a new NFT from the master copy.
    #[instruction]
    pub fn print_nft(
        #[account(mut, signer)] master_holding: AccountWithMetadata,
        #[account(init, signer)] printed_copy_holding: AccountWithMetadata,
    ) -> SpelResult {
        Ok(SpelOutput::execute(
            vec![master_holding, printed_copy_holding],
            vec![],
        ))
    }

    /// Rotate the recorded authority on a Token Definition to a new admin
    /// (LP-0013 / RFP-001). Panics with `Unauthorized` if the supplied
    /// authority does not match the current admin, or `Renounced` if the
    /// authority has been revoked.
    #[instruction]
    pub fn rotate_authority(
        #[account(mut)] definition: AccountWithMetadata,
        #[account(signer)] current_authority: AccountWithMetadata,
        new_admin: [u8; 32],
    ) -> SpelResult {
        Ok(SpelOutput::execute(
            vec![definition, current_authority],
            vec![],
        ))
    }

    /// Terminally renounce the recorded authority on a Token Definition
    /// (LP-0013 / RFP-001). Subsequent mint_with_authority / rotate_authority
    /// / revoke_authority calls panic with `Renounced`.
    #[instruction]
    pub fn revoke_authority(
        #[account(mut)] definition: AccountWithMetadata,
        #[account(signer)] current_authority: AccountWithMetadata,
    ) -> SpelResult {
        Ok(SpelOutput::execute(
            vec![definition, current_authority],
            vec![],
        ))
    }
}

// ── Account-data types (mirrors of `token_core` shapes) ───────────────────
//
// Re-declared here rather than imported from `programs/token/core/src/lib.rs`
// because the sidecar lives outside the main Cargo workspace (see
// `docs/SPEL_STATUS.md` — `nssa_core` v0.1.0 vs v0.2.0-rc3 collision).
// Any change to the canonical shapes must be mirrored here to keep the
// emitted IDL aligned.

#[account_type]
pub enum TokenDefinition {
    Fungible {
        name: String,
        total_supply: u128,
        metadata_id: [u8; 32],
        authority: [u8; 32],
    },
    NonFungible {
        name: String,
        printable_supply: u128,
        metadata_id: [u8; 32],
    },
}

#[account_type]
pub enum TokenHolding {
    Fungible {
        definition_id: [u8; 32],
        balance: u128,
    },
    NftMaster {
        definition_id: [u8; 32],
        print_balance: u128,
    },
    NftPrintedCopy {
        definition_id: [u8; 32],
        owned: bool,
    },
}

#[account_type]
pub struct TokenMetadata {
    pub definition_id: [u8; 32],
    pub standard: MetadataStandard,
    pub uri: String,
    pub creators: String,
    pub primary_sale_date: u64,
}

#[account_type]
pub enum MetadataStandard {
    Simple,
    Expanded,
}

#[account_type]
pub enum NewTokenDefinition {
    Fungible { name: String, total_supply: u128 },
    NonFungible { name: String, printable_supply: u128 },
}

#[account_type]
pub struct NewTokenMetadata {
    pub standard: MetadataStandard,
    pub uri: String,
    pub creators: String,
}
