//! This crate contains core data structures and utilities for the Token Program.

use borsh::{BorshDeserialize, BorshSerialize};
// Re-export the approval primitives so downstream Token program callers don't
// need to depend on `lez-approval` directly to construct/inspect authorities or
// match on the panic-payload error variants.
pub use lez_approval::{ApprovalError, Authority};
use nssa_core::account::{AccountId, Data};
use serde::{Deserialize, Serialize};

/// Token Program Instruction.
#[derive(Serialize, Deserialize)]
pub enum Instruction {
    /// Transfer tokens from sender to recipient.
    ///
    /// Required accounts:
    /// - Sender's Token Holding account (initialized, authorized),
    /// - Recipient's Token Holding account (initialized or authorized and uninitialized).
    Transfer { amount_to_transfer: u128 },

    /// Create a new fungible token definition without metadata.
    ///
    /// Required accounts:
    /// - Token Definition account (uninitialized, authorized),
    /// - Token Holding account (uninitialized, authorized).
    NewFungibleDefinition { name: String, total_supply: u128 },

    /// Create a new fungible token definition without metadata, with a mint authority
    /// that gates subsequent [`Self::MintWithAuthority`] calls (LP-0013 / RFP-001).
    ///
    /// `mint_authority` is the `AccountId` whose authorization claim must be present
    /// on every subsequent mint. `None` is a permanent-revocation marker that disables
    /// further minting from definition creation onward.
    ///
    /// The first time the `mint_authority` account signs a `MintWithAuthority`,
    /// `RotateAuthority`, or `RevokeAuthority` instruction, the Token program
    /// claims ownership of it (validator rule 7 requires non-default-owner once
    /// the account has tx history). Use a dedicated keypair per mint authority;
    /// do not reuse an existing wallet account, because after the claim the
    /// account can no longer sign for other programs.
    ///
    /// Required accounts:
    /// - Token Definition account (uninitialized, authorized),
    /// - Token Holding account (uninitialized, authorized).
    NewFungibleDefinitionWithAuthority {
        name: String,
        total_supply: u128,
        mint_authority: Option<AccountId>,
    },

    /// Create a new fungible or non-fungible token definition with metadata.
    ///
    /// Required accounts:
    /// - Token Definition account (uninitialized, authorized),
    /// - Token Holding account (uninitialized, authorized),
    /// - Token Metadata account (uninitialized, authorized).
    NewDefinitionWithMetadata {
        new_definition: NewTokenDefinition,
        /// Boxed to avoid large enum variant size.
        metadata: Box<NewTokenMetadata>,
    },

    /// Initialize a token holding account for a given token definition.
    ///
    /// Required accounts:
    /// - Token Definition account (initialized, any authorization),
    /// - Token Holding account (uninitialized, authorized),
    InitializeAccount,

    /// Burn tokens from the holder's account.
    ///
    /// Required accounts:
    /// - Token Definition account (initialized, any authorization),
    /// - Token Holding account (initialized, authorized).
    Burn { amount_to_burn: u128 },

    /// Mint new tokens to the holder's account.
    ///
    /// Required accounts:
    /// - Token Definition account (initialized, authorized),
    /// - Token Holding account (uninitialized or authorized and initialized).
    Mint { amount_to_mint: u128 },

    /// Mint new tokens, gated by the authority recorded on the Token Definition
    /// (LP-0013 / RFP-001). Panics with [`ApprovalError::Unauthorized`] if the
    /// authority account is not authorized or its id does not match the recorded
    /// authority. Panics with [`ApprovalError::Renounced`] if the definition's
    /// authority has been renounced.
    ///
    /// Required accounts (order matters):
    /// - Token Definition account (initialized, any authorization),
    /// - Token Holding account (uninitialized or authorized and initialized),
    /// - Mint Authority account (must be authorized; its id must equal the active admin in the
    ///   `authority` field on the Token Definition). Claimed by the Token program on first use —
    ///   see [`Self::NewFungibleDefinitionWithAuthority`].
    MintWithAuthority { amount_to_mint: u128 },

    /// Print a new NFT from the master copy.
    ///
    /// Required accounts:
    /// - NFT Master Token Holding account (authorized),
    /// - NFT Printed Copy Token Holding account (uninitialized, authorized).
    PrintNft,

    /// Rotate the recorded authority on a Token Definition to `new_admin`
    /// (LP-0013 / RFP-001). Panics with [`ApprovalError::Renounced`] if the
    /// authority has been revoked, or [`ApprovalError::Unauthorized`] if the
    /// supplied authority account does not match the current admin.
    ///
    /// `new_admin` is carried as an instruction argument rather than as an
    /// account so the wire shape stays at two accounts (mirrors the
    /// `mint_authority` arg of [`Self::NewFungibleDefinitionWithAuthority`]).
    ///
    /// Required accounts (order matters):
    /// - Token Definition account (initialized, any authorization),
    /// - Current Authority account (must be authorized; its id must equal the active admin in the
    ///   `authority` field on the Token Definition). Claimed by the Token program on first use —
    ///   see [`Self::NewFungibleDefinitionWithAuthority`].
    RotateAuthority { new_admin: AccountId },

    /// Terminally renounce the recorded authority on a Token Definition
    /// (LP-0013 / RFP-001). Once renounced, subsequent
    /// [`Self::MintWithAuthority`], [`Self::RotateAuthority`], and
    /// [`Self::RevokeAuthority`] calls panic with [`ApprovalError::Renounced`].
    ///
    /// Required accounts (order matters):
    /// - Token Definition account (initialized, any authorization),
    /// - Current Authority account (must be authorized; its id must equal the active admin in the
    ///   `authority` field on the Token Definition). Claimed by the Token program on first use —
    ///   see [`Self::NewFungibleDefinitionWithAuthority`].
    RevokeAuthority,
}

#[derive(Serialize, Deserialize)]
pub enum NewTokenDefinition {
    Fungible {
        name: String,
        total_supply: u128,
    },
    NonFungible {
        name: String,
        printable_supply: u128,
    },
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum TokenDefinition {
    Fungible {
        name: String,
        total_supply: u128,
        metadata_id: Option<AccountId>,
        /// Single-admin authority permitted to mint additional supply (LP-0013 /
        /// RFP-001), expressed via the agnostic [`Authority`] primitive from the
        /// `lez-approval` crate. [`Authority::renounced`] for legacy definitions
        /// created via [`Instruction::NewFungibleDefinition`] or after the
        /// authority has been revoked. The presence of an authority does NOT
        /// gate the existing [`Instruction::Mint`] path; only
        /// [`Instruction::MintWithAuthority`] checks this field.
        ///
        /// NOTE: this field is a breaking change to the Borsh layout of pre-existing
        /// `TokenDefinition::Fungible` accounts. LEZ has no backward-compat guarantee
        /// for in-flight schema changes; see solution README for the deferred
        /// `FungibleV2` / separate-PDA alternatives.
        authority: Authority,
    },
    NonFungible {
        name: String,
        printable_supply: u128,
        metadata_id: AccountId,
    },
}

impl TryFrom<&Data> for TokenDefinition {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        Self::try_from_slice(data.as_ref())
    }
}

impl From<&TokenDefinition> for Data {
    fn from(definition: &TokenDefinition) -> Self {
        // Using size_of_val as size hint for Vec allocation
        let mut data = Vec::with_capacity(std::mem::size_of_val(definition));

        BorshSerialize::serialize(definition, &mut data)
            .expect("Serialization to Vec should not fail");

        Self::try_from(data).expect("Token definition encoded data should fit into Data")
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum TokenHolding {
    Fungible {
        definition_id: AccountId,
        balance: u128,
    },
    NftMaster {
        definition_id: AccountId,
        /// The amount of printed copies left - 1 (1 reserved for master copy itself).
        print_balance: u128,
    },
    NftPrintedCopy {
        definition_id: AccountId,
        /// Whether nft is owned by the holder.
        owned: bool,
    },
}

impl TokenHolding {
    #[must_use]
    pub const fn zeroized_clone_from(other: &Self) -> Self {
        match other {
            Self::Fungible { definition_id, .. } => Self::Fungible {
                definition_id: *definition_id,
                balance: 0,
            },
            Self::NftMaster { definition_id, .. } => Self::NftMaster {
                definition_id: *definition_id,
                print_balance: 0,
            },
            Self::NftPrintedCopy { definition_id, .. } => Self::NftPrintedCopy {
                definition_id: *definition_id,
                owned: false,
            },
        }
    }

    #[must_use]
    pub const fn zeroized_from_definition(
        definition_id: AccountId,
        definition: &TokenDefinition,
    ) -> Self {
        match definition {
            TokenDefinition::Fungible { .. } => Self::Fungible {
                definition_id,
                balance: 0,
            },
            TokenDefinition::NonFungible { .. } => Self::NftPrintedCopy {
                definition_id,
                owned: false,
            },
        }
    }

    #[must_use]
    pub const fn definition_id(&self) -> AccountId {
        match self {
            Self::Fungible { definition_id, .. }
            | Self::NftMaster { definition_id, .. }
            | Self::NftPrintedCopy { definition_id, .. } => *definition_id,
        }
    }
}

impl TryFrom<&Data> for TokenHolding {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        Self::try_from_slice(data.as_ref())
    }
}

impl From<&TokenHolding> for Data {
    fn from(holding: &TokenHolding) -> Self {
        // Using size_of_val as size hint for Vec allocation
        let mut data = Vec::with_capacity(std::mem::size_of_val(holding));

        BorshSerialize::serialize(holding, &mut data)
            .expect("Serialization to Vec should not fail");

        Self::try_from(data).expect("Token holding encoded data should fit into Data")
    }
}

#[derive(Serialize, Deserialize)]
pub struct NewTokenMetadata {
    /// Metadata standard.
    pub standard: MetadataStandard,
    /// Pointer to off-chain metadata.
    pub uri: String,
    /// Creators of the token.
    pub creators: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TokenMetadata {
    /// Token Definition account id.
    pub definition_id: AccountId,
    /// Metadata standard .
    pub standard: MetadataStandard,
    /// Pointer to off-chain metadata.
    pub uri: String,
    /// Creators of the token.
    pub creators: String,
    /// Block id of primary sale.
    pub primary_sale_date: u64,
}

/// Metadata standard defining the expected format of JSON located off-chain.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum MetadataStandard {
    Simple,
    Expanded,
}

impl TryFrom<&Data> for TokenMetadata {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        Self::try_from_slice(data.as_ref())
    }
}

impl From<&TokenMetadata> for Data {
    fn from(metadata: &TokenMetadata) -> Self {
        // Using size_of_val as size hint for Vec allocation
        let mut data = Vec::with_capacity(std::mem::size_of_val(metadata));

        BorshSerialize::serialize(metadata, &mut data)
            .expect("Serialization to Vec should not fail");

        Self::try_from(data).expect("Token metadata encoded data should fit into Data")
    }
}
