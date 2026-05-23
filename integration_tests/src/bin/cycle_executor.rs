//! LP-0013 cycle-cost executor bench.
//!
//! Runs the Token guest ELF under `risc0_zkvm::default_executor` (no proving)
//! against minimal synthetic pre-states for each of the three LP-0013
//! instructions (`MintWithAuthority`, `RotateAuthority`, `RevokeAuthority`)
//! and prints `SessionInfo::{user_cycles, total_cycles}` per instruction.
//!
//! Cycle counts are the RISC0-native execution-cost proxy on LEZ — LEZ does
//! not currently expose a Solana-style per-instruction compute-unit metric.
//! Cycle counts are deterministic for a given guest ELF + input.
//!
//! Run:
//!   `cargo run --release --bin cycle_executor -p integration_tests`.

#![expect(clippy::print_stdout, reason = "bench bin writes results to stdout")]

use nssa::program_methods::{TOKEN_ELF, TOKEN_ID};
use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::ProgramId,
};
use risc0_zkvm::{ExecutorEnv, default_executor, serde::to_vec};
use token_core::{Authority, Instruction, TokenDefinition};

fn definition_with_authority(
    definition_id: AccountId,
    authority: Authority,
) -> AccountWithMetadata {
    AccountWithMetadata {
        account: Account {
            program_owner: TOKEN_ID,
            balance: 0,
            data: Data::from(&TokenDefinition::Fungible {
                name: String::from("bench"),
                total_supply: 100_000_u128,
                metadata_id: None,
                authority,
            }),
            nonce: 0_u128.into(),
        },
        is_authorized: false,
        account_id: definition_id,
    }
}

fn authority_signer(authority_id: AccountId, is_authorized: bool) -> AccountWithMetadata {
    AccountWithMetadata {
        account: Account::default(),
        is_authorized,
        account_id: authority_id,
    }
}

fn empty_holding(holding_id: AccountId) -> AccountWithMetadata {
    AccountWithMetadata {
        account: Account::default(),
        is_authorized: true,
        account_id: holding_id,
    }
}

fn execute(
    label: &str,
    program_id: ProgramId,
    pre_states: &[AccountWithMetadata],
    instruction: &Instruction,
) -> anyhow::Result<()> {
    let instruction_words = to_vec(instruction)?;
    let caller: Option<ProgramId> = None;
    let pre_states_vec = pre_states.to_vec();

    let mut builder = ExecutorEnv::builder();
    builder
        .write(&program_id)?
        .write(&caller)?
        .write(&pre_states_vec)?
        .write(&instruction_words)?;
    let env = builder.build()?;

    let executor = default_executor();
    let session = executor
        .execute(env, TOKEN_ELF)
        .map_err(|e| anyhow::anyhow!("execute({label}) failed: {e}"))?;

    let user_cycles = session.cycles();
    let padded_cycles: u64 = session.segments.iter().map(|s| 1_u64 << s.po2).sum();
    let journal_bytes = session.journal.bytes.len();
    let segments = session.segments.len();

    println!(
        "{label:>32}  user_cycles={user_cycles:>10}  padded_cycles={padded_cycles:>10}  segments={segments:>2}  journal={journal_bytes}B"
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let program_id = TOKEN_ID;

    // Stable synthetic ids — same numbering used by `programs/token/src/tests.rs`.
    let definition_id = AccountId::new([15; 32]);
    let admin_id = AccountId::new([20; 32]);
    let new_admin_id = AccountId::new([21; 32]);
    let holding_id = AccountId::new([17; 32]);

    println!(
        "--- LP-0013 cycle costs (default_executor, RISC0_DEV_MODE={}) ---",
        std::env::var("RISC0_DEV_MODE").unwrap_or_else(|_| String::from("unset"))
    );
    println!("ELF size: {} bytes", TOKEN_ELF.len());
    println!("ELF sha256: {}", hex_sha256(TOKEN_ELF));
    println!();

    // ---- MintWithAuthority (happy path) -----------------------------------
    let def_mint = definition_with_authority(definition_id, Authority::new(admin_id));
    let holding_mint = empty_holding(holding_id);
    let authority_mint = authority_signer(admin_id, true);
    execute(
        "MintWithAuthority(happy)",
        program_id,
        &[def_mint, holding_mint, authority_mint],
        &Instruction::MintWithAuthority {
            amount_to_mint: 1_000,
        },
    )?;

    // ---- MintWithAuthority (rejected: Unauthorized) -----------------------
    let def_rej = definition_with_authority(definition_id, Authority::new(admin_id));
    let holding_rej = empty_holding(holding_id);
    let imposter = authority_signer(AccountId::new([99; 32]), true);
    let res = std::panic::catch_unwind(|| {
        execute(
            "MintWithAuthority(unauthorized)",
            program_id,
            &[def_rej, holding_rej, imposter],
            &Instruction::MintWithAuthority {
                amount_to_mint: 1_000,
            },
        )
    });
    if let Ok(Err(err)) = res {
        println!("  ↳ rejected as expected: {err}");
    }

    // ---- RotateAuthority (happy path) -------------------------------------
    let def_rot = definition_with_authority(definition_id, Authority::new(admin_id));
    let auth_rot = authority_signer(admin_id, true);
    execute(
        "RotateAuthority(happy)",
        program_id,
        &[def_rot, auth_rot],
        &Instruction::RotateAuthority {
            new_admin: new_admin_id,
        },
    )?;

    // ---- RevokeAuthority (happy path) -------------------------------------
    let def_rev = definition_with_authority(definition_id, Authority::new(admin_id));
    let auth_rev = authority_signer(admin_id, true);
    execute(
        "RevokeAuthority(happy)",
        program_id,
        &[def_rev, auth_rev],
        &Instruction::RevokeAuthority,
    )?;

    Ok(())
}

fn hex_sha256(data: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
