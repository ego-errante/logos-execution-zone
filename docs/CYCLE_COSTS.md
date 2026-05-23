# LP-0013 cycle costs (raw measurement log)

This file is the source-of-truth raw log behind the CU/cycle table in `README.md`.
Numbers are captured by `integration_tests/src/bin/cycle_executor.rs`, which loads
the committed `artifacts/program_methods/token.bin` ELF and runs it through
`risc0_zkvm::default_executor()` (executor only — no proving) for each LP-0013
instruction with the minimal synthetic pre-state set the dispatcher expects.

## Measurement conditions

| Field | Value |
|---|---|
| Date (UTC) | 2026-05-23 |
| Token program ELF | `artifacts/program_methods/token.bin` (461 740 bytes) |
| Token program ELF sha256 | `493ce4e7e902e54516feb9c8728a546750ca5b3a2e47759ec7768dd272b37222` |
| Repo HEAD at measurement | `5b9b947c68607d51e0443c0c46160ca36a81edf9` (`lp-0013-token-authorities`) |
| Host machine | Apple M1, 8 cores (4P + 4E), 8 GB RAM, macOS 15.5 (Darwin 24.5.0) |
| `risc0_zkvm` | 3.0.5 |
| `RISC0_DEV_MODE` | `0` (real cycle counting) |
| Profile | `--release` |

## Reproduction

```bash
RISC0_DEV_MODE=0 cargo run --release --bin cycle_executor -p integration_tests
```

## Raw output (2026-05-23)

```
--- LP-0013 cycle costs (default_executor, RISC0_DEV_MODE=0) ---
ELF size: 461740 bytes
ELF sha256: 493ce4e7e902e54516feb9c8728a546750ca5b3a2e47759ec7768dd272b37222

        MintWithAuthority(happy)  user_cycles=    154858  padded_cycles=    262144  segments= 1  journal=1344B
        ↳ MintWithAuthority(unauthorized) — guest panic at lez-approval/src/lib.rs:68:
            "not authorized: signer does not match admin authority"
          (executor returns Err; no SessionInfo / cycle count is produced for
          panics, but the rejection path is exercised end-to-end.)
          RotateAuthority(happy)  user_cycles=    127350  padded_cycles=    262144  segments= 1  journal=984B
          RevokeAuthority(happy)  user_cycles=    103913  padded_cycles=    262144  segments= 1  journal=808B
```

## How to read this

- **`user_cycles`** — the deterministic count of user-program cycles executed by
  the RISC-V interpreter. This is what `risc0_zkvm::SessionInfo::cycles()`
  returns: the sum of `cycles` over `SessionInfo::segments`. Reproducible across
  machines for the same ELF + input.
- **`padded_cycles`** — sum of `2^po2` over segments, i.e. what the prover
  would need to commit to after power-of-2 padding. Always rounds up to the
  next 2^N boundary; for these small inputs every operation fits in a single
  2^18 = 262 144-cycle segment.
- **`segments`** — number of execution segments. > 1 means the executor split
  the run across continuations (none of the LP-0013 ops require this).
- **`journal`** — bytes the guest committed to the public output journal.
  Includes the serialized `ProgramOutput` (instruction echo, pre-states,
  post-states).

## Caveats

1. **LEZ has no per-instruction "compute unit" (CU) metric** in the Solana
   sense. The Token program runs end-to-end inside one RISC0 guest invocation;
   the natural granularity of execution cost is the RISC0 user-cycle count.
   Cycle counts are the LEZ-native proxy.
2. **Cycle counts are deterministic for a given ELF + input**, but wall-clock
   proof time is machine-dependent. This bench measures executor cycles, not
   prover wall-clock. For wall-clock proof times on a live sequencer, the
   existing standalone-sequencer harness (`demo.sh`) emits `RUST_LOG=info`
   timing on each privacy-preserving-circuit prove call.
3. **Rejected paths do not produce cycle counts.** When the guest panics (e.g.
   `ApprovalError::Unauthorized`, `ApprovalError::Renounced`), the executor
   returns `Err` rather than a `SessionInfo`. The rejection is observable as
   end-to-end behavior (the demo / integration test asserts on persisted
   state staying unchanged) but the partial cycle work is not recoverable
   through the public RISC0 v3.0.5 API on the executor path.
4. **Synthetic minimum inputs.** The bench uses the smallest pre-state set
   the dispatcher accepts: a `TokenDefinition::Fungible` with a single
   `Authority::new(admin_id)`, a default-initialised holding account
   (for `MintWithAuthority`), and a single `authority_signer` account with
   `is_authorized: true`. Real production transactions will have approximately
   the same cycle cost since the on-chain shape is the same; the only variable
   is `name: String` (which we set to `"bench"` here).
