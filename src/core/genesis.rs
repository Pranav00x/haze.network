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
// KNOWN, DISCLOSED GAP: the team/advisor and investor allocations below are
// minted as ordinary, IMMEDIATELY SPENDABLE genesis outputs, exactly like
// every other output in this protocol - there is no timelock/vesting
// primitive here. Shipping this to a real mainnet without real vesting
// (either a protocol-level timelock feature, or a legal multisig/escrow
// structure held outside the chain) is a known risk, not something this
// code has solved. Do not treat this constant existing as "vesting handled."
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
pub const TEAM_BLINDING: u64 = 44;
pub const INVESTOR_BLINDING: u64 = 45;
pub const AIRDROP_BLINDING: u64 = 46;
pub const TREASURY_BLINDING: u64 = 43; // same secret the old FAUCET_RESERVE_BLINDING used

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

/// Computes and returns the hardcoded Genesis block for Haze. Mints five
/// known outputs: the validator stake / claim-genesis output (1,000,000,
/// blinding=42, unchanged since before the tokenomics lock), and the four
/// allocation outputs above (team, investor, airdrop, treasury).
pub fn genesis_block() -> Block {
    let genesis_val = 1_000_000u64;
    let genesis_blinding = Scalar::from(42u64);

    let (validator_output, validator_kernel) = mint_output(genesis_val, genesis_blinding);
    let (team_output, team_kernel) = mint_output(TEAM_ALLOCATION, Scalar::from(TEAM_BLINDING));
    let (investor_output, investor_kernel) = mint_output(INVESTOR_ALLOCATION, Scalar::from(INVESTOR_BLINDING));
    let (airdrop_output, airdrop_kernel) = mint_output(AIRDROP_ALLOCATION, Scalar::from(AIRDROP_BLINDING));
    let (treasury_output, treasury_kernel) = mint_output(TREASURY_ALLOCATION, Scalar::from(TREASURY_BLINDING));

    let body = Transaction {
        inputs: vec![],
        outputs: vec![validator_output, team_output, investor_output, airdrop_output, treasury_output],
        kernels: vec![validator_kernel, team_kernel, investor_kernel, airdrop_kernel, treasury_kernel],
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
