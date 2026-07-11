//! Consensus-level timelock for the team/investor genesis allocations (see
//! core::genesis's tokenomics lock). Enforces a 6-month cliff followed by a
//! 2-year, 7-tranche quarterly vest: any block that spends one of these
//! specific tranche outputs before its tranche's unlock height is rejected
//! outright by ChainState::apply_linear_block - the same call site that
//! already enforces chain_id and range proofs.
//!
//! This module only enforces WHEN a tranche can be spent, not WHO can spend
//! it - see core::genesis's module doc comment for how the blinding
//! secrets backing these tranches are kept out of this repo entirely.
use crate::transaction::Input;
use haze_crypto::pedersen::Commitment;

/// 365 days at this chain's 10s block time (see core::genesis's halving
/// schedule for the same block-time assumption).
pub const BLOCKS_PER_YEAR: u64 = 3_155_760;
pub const VESTING_CLIFF_BLOCKS: u64 = BLOCKS_PER_YEAR / 2; // 6 months
pub const VESTING_TRANCHE_INTERVAL_BLOCKS: u64 = BLOCKS_PER_YEAR / 4; // quarterly

/// Unlock height of the tranche at `tranche_index` (0-based): index 0 unlocks
/// at the 6-month cliff, each subsequent index 3 months later, through index
/// 6 (24 months) - see core::genesis::VESTING_TRANCHE_COUNT.
pub fn tranche_unlock_height(tranche_index: usize) -> u64 {
    VESTING_CLIFF_BLOCKS + (tranche_index as u64) * VESTING_TRANCHE_INTERVAL_BLOCKS
}

/// Every (commitment, unlock_height) pair for the locked genesis tranches -
/// team and investor allocations only (airdrop/treasury are intentionally
/// not locked, see core::genesis). Recomputed on demand rather than cached:
/// it's only ~14 curve operations and this only runs once per block apply.
pub fn locked_genesis_outputs() -> Vec<(Commitment, u64)> {
    let team = &super::genesis::TEAM_TRANCHES;
    let investor = &super::genesis::INVESTOR_TRANCHES;
    let mut locked = Vec::with_capacity(team.len() + investor.len());

    for (i, data) in team.iter().enumerate() {
        locked.push((data.commitment(), tranche_unlock_height(i)));
    }
    for (i, data) in investor.iter().enumerate() {
        locked.push((data.commitment(), tranche_unlock_height(i)));
    }

    locked
}

/// True if applying a block at `block_height` would spend any locked tranche
/// before its unlock height - if so, the whole block must be rejected (see
/// ChainState::apply_linear_block).
pub fn spends_locked_output_early(inputs: &[Input], block_height: u64) -> bool {
    let locked = locked_genesis_outputs();
    inputs.iter().any(|input| {
        locked.iter().any(|(commitment, unlock_height)| {
            input.commitment == *commitment && block_height < *unlock_height
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tranche_unlock_heights_match_the_6mo_cliff_2yr_quarterly_schedule() {
        // 6, 9, 12, 15, 18, 21, 24 months.
        assert_eq!(tranche_unlock_height(0), VESTING_CLIFF_BLOCKS);
        assert_eq!(tranche_unlock_height(6), VESTING_CLIFF_BLOCKS + 6 * VESTING_TRANCHE_INTERVAL_BLOCKS);
        assert_eq!(tranche_unlock_height(6), BLOCKS_PER_YEAR * 2, "the 7th and final tranche must unlock at exactly 2 years");
    }

    #[test]
    fn locked_genesis_outputs_covers_every_team_and_investor_tranche() {
        let locked = locked_genesis_outputs();
        assert_eq!(
            locked.len(),
            super::super::genesis::TEAM_TRANCHES.len() + super::super::genesis::INVESTOR_TRANCHES.len()
        );
    }

    #[test]
    fn spends_locked_output_early_rejects_before_unlock_and_allows_after() {
        let locked = locked_genesis_outputs();
        let (commitment, unlock_height) = locked[0];

        let inputs = vec![Input { commitment }];
        assert!(spends_locked_output_early(&inputs, 0), "spending before the cliff must be flagged");
        assert!(spends_locked_output_early(&inputs, unlock_height - 1), "spending one block before unlock must be flagged");
        assert!(!spends_locked_output_early(&inputs, unlock_height), "spending exactly at the unlock height must be allowed");
        assert!(!spends_locked_output_early(&inputs, unlock_height + 1), "spending after the unlock height must be allowed");
    }

    #[test]
    fn spends_locked_output_early_ignores_unrelated_commitments() {
        use curve25519_dalek_ng::scalar::Scalar;
        let unrelated = Commitment::new(123, Scalar::from(999u64));
        let inputs = vec![Input { commitment: unrelated }];
        assert!(!spends_locked_output_early(&inputs, 0), "an ordinary, non-genesis commitment must never be treated as locked");
    }
}
