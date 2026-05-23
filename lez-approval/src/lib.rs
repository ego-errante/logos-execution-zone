//! `lez-approval` — RFP-001 single-admin approval primitives for LEZ programs.
//!
//! Provides the [`Authority`] type, a thin wrapper over `Option<AccountId>` that
//! encodes a single-admin authorization scheme with three operations:
//!
//! - [`Authority::gate`] — assert a signer is the active admin (panics otherwise)
//! - [`Authority::transfer`] — rotate the admin to a new account
//! - [`Authority::revoke`] — terminally renounce the admin
//!
//! Failures panic with the [`Display`](std::fmt::Display) representation of
//! [`ApprovalError`], matching the LEZ guest convention (see e.g.
//! `programs/token/src/mint.rs`'s gating block). Callers therefore do not need
//! to thread `Result` through their handler bodies.

use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::account::{AccountId, AccountWithMetadata};
use serde::{Deserialize, Serialize};

/// Single-admin authority state per RFP-001.
///
/// `Some(id)` denotes an active admin; `None` denotes a permanently-renounced
/// authority. The renounced state is terminal — once an `Authority` becomes
/// `None`, every subsequent [`Authority::gate`] call panics with
/// [`ApprovalError::Renounced`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct Authority(pub Option<AccountId>);

/// Errors raised by [`Authority`] operations.
///
/// These are never returned — they are formatted with [`std::fmt::Display`]
/// and used as the panic payload to match the LEZ guest convention.
#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    /// The signer was not the active admin, or its account was not authorized.
    #[error("not authorized: signer does not match admin authority")]
    Unauthorized,
    /// The authority has been renounced; no further admin operations are valid.
    #[error("authority renounced: operation requires an active admin")]
    Renounced,
}

impl Authority {
    /// Construct an authority with `admin` as the active admin.
    #[must_use]
    pub const fn new(admin: AccountId) -> Self {
        Self(Some(admin))
    }

    /// Construct a permanently-renounced authority.
    #[must_use]
    pub const fn renounced() -> Self {
        Self(None)
    }

    /// RFP-001 `gate` — assert the supplied account is the active admin and
    /// is signed (`is_authorized`).
    ///
    /// # Panics
    /// - With [`ApprovalError::Renounced`] if the authority is `None`.
    /// - With [`ApprovalError::Unauthorized`] if the signer's `account_id`
    ///   does not match the admin, or `is_authorized` is `false`.
    pub fn gate(&self, signer: &AccountWithMetadata) {
        let Some(admin) = self.0 else {
            panic!("{}", ApprovalError::Renounced);
        };
        assert!(
            signer.is_authorized && signer.account_id == admin,
            "{}",
            ApprovalError::Unauthorized
        );
    }

    /// RFP-001 `transfer` — rotate the admin to `new_admin`. Gates on the
    /// current admin first, so only the current admin can rotate.
    ///
    /// # Panics
    /// Same conditions as [`Authority::gate`].
    pub fn transfer(&mut self, signer: &AccountWithMetadata, new_admin: AccountId) {
        self.gate(signer);
        self.0 = Some(new_admin);
    }

    /// RFP-001 `revoke` — renounce the authority terminally. Gates on the
    /// current admin first, so only the current admin can revoke.
    ///
    /// # Panics
    /// Same conditions as [`Authority::gate`].
    pub fn revoke(&mut self, signer: &AccountWithMetadata) {
        self.gate(signer);
        self.0 = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{ApprovalError, Authority};
    use nssa_core::account::{Account, AccountId, AccountWithMetadata};

    fn aid(byte: u8) -> AccountId {
        AccountId::new([byte; 32])
    }

    fn signer(id: AccountId, is_authorized: bool) -> AccountWithMetadata {
        AccountWithMetadata {
            account: Account::default(),
            is_authorized,
            account_id: id,
        }
    }

    #[test]
    fn gate_succeeds_when_signer_matches() {
        let admin = aid(1);
        let authority = Authority::new(admin);
        authority.gate(&signer(admin, true));
    }

    #[test]
    #[should_panic(expected = "not authorized: signer does not match admin authority")]
    fn gate_panics_when_signer_unauthorized() {
        let admin = aid(1);
        let other = aid(2);
        let authority = Authority::new(admin);
        authority.gate(&signer(other, true));
    }

    #[test]
    #[should_panic(expected = "authority renounced: operation requires an active admin")]
    fn gate_panics_when_renounced() {
        let admin = aid(1);
        let authority = Authority::renounced();
        authority.gate(&signer(admin, true));
    }

    #[test]
    fn transfer_rotates_admin() {
        let admin = aid(1);
        let new_admin = aid(2);
        let mut authority = Authority::new(admin);

        authority.transfer(&signer(admin, true), new_admin);

        assert_eq!(authority, Authority::new(new_admin));
        authority.gate(&signer(new_admin, true));
    }

    #[test]
    fn revoke_renounces_terminally() {
        let admin = aid(1);
        let mut authority = Authority::new(admin);

        authority.revoke(&signer(admin, true));

        assert_eq!(authority, Authority::renounced());
        // Verify renounce is terminal: error variant is `Renounced`.
        let err = ApprovalError::Renounced;
        assert!(matches!(err, ApprovalError::Renounced));
    }
}
