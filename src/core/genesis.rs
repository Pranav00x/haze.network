use crate::core::block::{Block, BlockHeader};
use crate::core::transaction::{Transaction, Output, TxKernel};
use crate::crypto::pedersen::Commitment;
use crate::crypto::range_proof::RangeProof;
use crate::crypto::schnorr::Signature;
use curve25519_dalek_ng::scalar::Scalar;

// ---------------------------------------------------------------------------
// Tokenomics - LOCKED. Changing anything below (supply, split, halving
// schedule, or genesis output serialization) is a hard fork of this chain's
// history; treat it as a reset, not a tweak.
//
// Total supply target: ~21,000,000,000 HAZE (whole units - this protocol has
// no decimal subdivision, unlike Bitcoin's satoshis). A literal 21,000,000
// cap (Bitcoin's actual number) can't survive a multi-year halving schedule
// without the per-block reward rounding to zero almost immediately - at this
// chain's ~4-year halving cadence, 21,000,000 total works out to well under
// 1 whole HAZE per block from genesis. 21 BILLION keeps whole-number rewards
// meaningful for the schedule's full ~40-year tail while keeping the "21"
// figure. The exact realized total is an emergent consequence of the clean
// primary constants below (HALVING_INTERVAL_BLOCKS, INITIAL_BLOCK_REWARD),
// not something back-solved to hit 21,000,000,000 exactly - same as
// Bitcoin's own 21,000,000 is a consequence of 50 BTC / 210,000 blocks, not
// the other way around.
//
// Split: 65% emitted via block rewards over time (see
// core::block::block_reward_at), 35% minted at genesis (13% team/advisors,
// 13% investors, 6% airdrop, 3% treasury).
//
// Team and investor allocations vest via a protocol-level timelock (see
// core::vesting): a 6-month cliff, then 7 quarterly tranches through 2
// years total. Any block spending a tranche before its unlock height is
// rejected by ChainState::apply_linear_block.
//
// KNOWN, DISCLOSED GAP the timelock does NOT solve: the blinding factors
// backing every tranche (TEAM_BLINDING_TRANCHES / INVESTOR_BLINDING_TRANCHES
// below) are small, hardcoded integers committed to this public repo - the
// same devnet convention as the genesis claim blinding (=42), fine for
// devnet/testnet, but a timelock on a publicly-known secret only delays a
// theft until the unlock height, it doesn't prevent one. Before any of this
// holds real value, those blindings must be replaced with genuinely random
// secrets generated and held privately by whoever actually controls
// team/investor funds - that's custody, not something this code can solve.
pub const TOTAL_SUPPLY_TARGET: u64 = 21_000_000_000;

/// Block-reward halving schedule (~65% of TOTAL_SUPPLY_TARGET, emitted over
/// time - see core::block::block_reward_at). 12,600,000 blocks is ~4 years
/// at this chain's 10s block time (12_600_000 * 10s ≈ 3.994 years),
/// matching Bitcoin's own halving cadence in wall-clock time rather than
/// block count (its 210,000-block interval is calibrated to 10-minute
/// blocks and has no meaning here). 540 halved 10 times reaches 0
/// (540,270,135,67,33,16,8,4,2,1,0), so full emission tapers out after
/// ~126,000,000 blocks (~40 years) - summing to ~13,557,600,000 HAZE, close
/// to (not exactly) the 65% target, same as Bitcoin's real total isn't
/// exactly 21,000,000 either.
pub const HALVING_INTERVAL_BLOCKS: u64 = 12_600_000;
pub const INITIAL_BLOCK_REWARD: u64 = 540;

pub const TEAM_ALLOCATION: u64 = 2_730_000_000;
pub const INVESTOR_ALLOCATION: u64 = 2_730_000_000;
pub const AIRDROP_ALLOCATION: u64 = 1_260_000_000;
/// Also what funds the node's own repeatable devnet faucet (see
/// src/api/faucet.rs) - the old dedicated 50,000,000 faucet-only mint is
/// gone; the faucet now draws from this same treasury allocation instead of
/// a separate ad-hoc pre-mine.
pub const TREASURY_ALLOCATION: u64 = 630_000_000;

/// Total minted at genesis outside the block-reward schedule: the four
/// allocations above, plus the pre-existing 1,000,000 validator-stake /
/// claim-genesis convenience output (untouched by this reallocation - its
/// value and blinding=42 are hardcoded directly into consensus in several
/// places, see core::chain::select_proposer/apply_linear_block, so it isn't
/// part of the tokenomics split above; it's bootstrap plumbing, not supply).
pub const GENESIS_TOTAL_MINTED: u64 = 1_000_000
    + TEAM_ALLOCATION + INVESTOR_ALLOCATION + AIRDROP_ALLOCATION + TREASURY_ALLOCATION;

/// Well-known devnet blinding secrets for each genesis allocation output -
/// same convention as the untouched validator-stake output (blinding=42).
pub const AIRDROP_BLINDING: u64 = 46;
pub const TREASURY_BLINDING: u64 = 43; // same secret the old FAUCET_RESERVE_BLINDING used

/// Team and investor allocations are each split into VESTING_TRANCHE_COUNT
/// equal tranches, one per vesting unlock height (see core::vesting) -
/// TEAM_ALLOCATION / INVESTOR_ALLOCATION both divide evenly by this count.
pub const VESTING_TRANCHE_COUNT: usize = 7;
pub const TEAM_TRANCHE_VALUE: u64 = TEAM_ALLOCATION / VESTING_TRANCHE_COUNT as u64;
pub const INVESTOR_TRANCHE_VALUE: u64 = INVESTOR_ALLOCATION / VESTING_TRANCHE_COUNT as u64;

pub const TEAM_BLINDING_TRANCHES: [u64; VESTING_TRANCHE_COUNT] = [100, 101, 102, 103, 104, 105, 106];
pub const INVESTOR_BLINDING_TRANCHES: [u64; VESTING_TRANCHE_COUNT] = [110, 111, 112, 113, 114, 115, 116];

// ---------------------------------------------------------------------------
// Network identity - enforced in BlockHeader::hash() (see core::block) so
// nodes on different networks can never accidentally interoperate, even if
// they somehow connected over P2P.
pub const CHAIN_ID: u64 = 1;
pub const NETWORK_NAME: &str = "haze-testnet-1";

fn mint_output(value: u64, blinding: Scalar) -> (Output, TxKernel) {
    let commitment = Commitment::new(value, blinding);
    let proof = RangeProof::prove(value, &blinding);
    // No note: these are fixed, well-known devnet constants that every
    // wallet already hardcodes directly (see wasm::claim_genesis) rather
    // than deriving/discovering via the note-recovery mechanism.
    let output = Output { commitment, proof, note: vec![] };

    // No corresponding input, so the excess blinding factor is just the
    // negation of the output's own blinding factor.
    let excess_blinding = Scalar::zero() - blinding;
    let excess_commitment = Commitment::new(0, excess_blinding);
    let signature = Signature::sign(&0u64.to_le_bytes(), &excess_blinding);
    let kernel = TxKernel { excess: excess_commitment, fee: 0, signature };

    (output, kernel)
}

/// Computes and returns the hardcoded Genesis block for Haze. Mints 17
/// known outputs: the validator stake / claim-genesis output (1,000,000,
/// blinding=42, unchanged since before the tokenomics lock), 7 team vesting
/// tranches, 7 investor vesting tranches (see core::vesting for the
/// timelock enforcing when each can be spent), and the airdrop/treasury
/// allocations (unlocked, see the module doc comment for why).
pub fn genesis_block() -> Block {
    let genesis_val = 1_000_000u64;
    let genesis_blinding = Scalar::from(42u64);

    let (validator_output, validator_kernel) = mint_output(genesis_val, genesis_blinding);

    let mut outputs = vec![validator_output];
    let mut kernels = vec![validator_kernel];

    for blinding in TEAM_BLINDING_TRANCHES {
        let (output, kernel) = mint_output(TEAM_TRANCHE_VALUE, Scalar::from(blinding));
        outputs.push(output);
        kernels.push(kernel);
    }
    for blinding in INVESTOR_BLINDING_TRANCHES {
        let (output, kernel) = mint_output(INVESTOR_TRANCHE_VALUE, Scalar::from(blinding));
        outputs.push(output);
        kernels.push(kernel);
    }

    let (airdrop_output, airdrop_kernel) = mint_output(AIRDROP_ALLOCATION, Scalar::from(AIRDROP_BLINDING));
    let (treasury_output, treasury_kernel) = mint_output(TREASURY_ALLOCATION, Scalar::from(TREASURY_BLINDING));
    outputs.push(airdrop_output);
    kernels.push(airdrop_kernel);
    outputs.push(treasury_output);
    kernels.push(treasury_kernel);

    let body = Transaction {
        inputs: vec![],
        outputs,
        kernels,
    };

    Block {
        header: BlockHeader {
            height: 0,
            prev_hash: [0u8; 32],
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(genesis_val, genesis_blinding),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: super::registry::compute_registry_root(&std::collections::HashMap::new()),
            chain_id: CHAIN_ID,
        },
        body,
        name_ops: vec![],
        transfer_ops: vec![],
    }
}
