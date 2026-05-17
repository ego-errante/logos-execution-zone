//! This crate contains core data structures and utilities for the Token Program.

use borsh::{BorshDeserialize, BorshSerialize};
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

    /// Mint new tokens, gated by the mint authority recorded on the Token Definition
    /// (LP-0013 / RFP-001). Panics with [`TokenError::Unauthorized`] if the authority
    /// account is not authorized or its id does not match the recorded authority.
    /// Panics with [`TokenError::AuthorityRevoked`] if the definition's mint authority
    /// is `None`.
    ///
    /// Required accounts (order matters):
    /// - Token Definition account (initialized, any authorization),
    /// - Token Holding account (uninitialized or authorized and initialized),
    /// - Mint Authority account (must be authorized; its id must equal the
    ///   `mint_authority` field on the Token Definition).
    MintWithAuthority { amount_to_mint: u128 },

    /// Print a new NFT from the master copy.
    ///
    /// Required accounts:
    /// - NFT Master Token Holding account (authorized),
    /// - NFT Printed Copy Token Holding account (uninitialized, authorized).
    PrintNft,
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
        /// Account permitted to mint additional supply (LP-0013 / RFP-001).
        /// `None` for legacy definitions created via [`Instruction::NewFungibleDefinition`]
        /// or after the authority has been revoked. The presence of an authority does NOT
        /// gate the existing [`Instruction::Mint`] path; only [`Instruction::MintWithAuthority`]
        /// checks this field.
        ///
        /// NOTE: this field is a breaking change to the Borsh layout of pre-existing
        /// `TokenDefinition::Fungible` accounts. LEZ has no backward-compat guarantee
        /// for in-flight schema changes; see solution README for the deferred
        /// `FungibleV2` / separate-PDA alternatives.
        mint_authority: Option<AccountId>,
    },
    NonFungible {
        name: String,
        printable_supply: u128,
        metadata_id: AccountId,
    },
}

/// Deterministic error variants raised by the Token program when an instruction
/// fails a precondition. Used as the panic message body so the off-chain caller
/// can grep for a stable identifier; see [`Instruction::MintWithAuthority`] for
/// the authorization paths that emit these.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TokenError {
    /// The supplied authority account is not authorized, or its id does not match
    /// the `mint_authority` recorded on the Token Definition.
    Unauthorized,
    /// The Token Definition's `mint_authority` is `None` — minting via
    /// [`Instruction::MintWithAuthority`] is permanently disabled for this token.
    AuthorityRevoked,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => f.write_str("TokenError::Unauthorized"),
            Self::AuthorityRevoked => f.write_str("TokenError::AuthorityRevoked"),
        }
    }
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
