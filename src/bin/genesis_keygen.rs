//! One-time offline tool: generates real, cryptographically random blinding
//! secrets for every locked genesis allocation output (team/investor
//! tranches, airdrop, treasury) and prints two things:
//!
//!   1. The PUBLIC data (commitment + range proof + kernel excess/signature)
//!      ready to paste into core::genesis as hardcoded hex constants - this
//!      is what every node needs to independently reconstruct the exact
//!      same genesis block, and is safe to commit.
//!   2. The PRIVATE secret backing each output - printed once, here, and
//!      nowhere else. Copy these out immediately (password manager,
//!      hardware wallet, wherever) - this tool does not save them, and if
//!      you lose them before securing them elsewhere, the funds are gone.
//!
//! Never commit the secrets themselves. Only the "PUBLIC" block below
//! belongs in genesis.rs.
use curve25519_dalek_ng::scalar::Scalar;
use rand::rngs::OsRng;
use haze_core::crypto::pedersen::Commitment;
use haze_core::crypto::range_proof::RangeProof;
use haze_core::crypto::schnorr::Signature;

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn gen_locked_output(label: &str, value: u64) {
    let secret = Scalar::random(&mut OsRng);
    let commitment = Commitment::new(value, secret);
    let proof = RangeProof::prove(value, &secret);

    // Same kernel construction as genesis::mint_output: no corresponding
    // input, so the excess blinding factor is just the negation of the
    // output's own blinding factor.
    let excess_blinding = Scalar::zero() - secret;
    let excess_commitment = Commitment::new(0, excess_blinding);
    let signature = Signature::sign(&0u64.to_le_bytes(), &excess_blinding);

    println!("=== {} (value={}) ===", label, value);
    println!("PRIVATE secret (save NOW, never commit): {}", to_hex(secret.as_bytes()));
    println!("PUBLIC commitment_hex: {}", commitment.to_hex());
    println!("PUBLIC proof_hex:      {}", to_hex(&proof.0.to_bytes()));
    println!("PUBLIC excess_hex:     {}", excess_commitment.to_hex());
    println!("PUBLIC sig_s_hex:      {}", to_hex(signature.s.as_bytes()));
    println!("PUBLIC sig_e_hex:      {}", to_hex(signature.e.as_bytes()));
    println!();
}

fn main() {
    println!("Generating real genesis secrets - PRIVATE lines must be saved");
    println!("immediately and never committed anywhere. PUBLIC lines are what");
    println!("goes into core::genesis::LOCKED_OUTPUTS.\n");

    const TEAM_TRANCHE_VALUE: u64 = 390_000_000;
    const INVESTOR_TRANCHE_VALUE: u64 = 390_000_000;
    const AIRDROP_ALLOCATION: u64 = 1_260_000_000;
    const TREASURY_ALLOCATION: u64 = 630_000_000;

    for i in 0..7 {
        gen_locked_output(&format!("team tranche {}", i), TEAM_TRANCHE_VALUE);
    }
    for i in 0..7 {
        gen_locked_output(&format!("investor tranche {}", i), INVESTOR_TRANCHE_VALUE);
    }
    gen_locked_output("airdrop", AIRDROP_ALLOCATION);
    gen_locked_output("treasury (also set as HAZE_TREASURY_BLINDING env var wherever the faucet runs)", TREASURY_ALLOCATION);
}
