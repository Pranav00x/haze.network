use curve25519_dalek_ng::scalar::Scalar;

use crate::crypto::pedersen::Commitment;
use crate::crypto::range_proof::RangeProof;
use crate::crypto::schnorr::Signature;
use crate::core::transaction::{Transaction, Input, Output, TxKernel};
use super::keystore::Keystore;
use super::note;
use super::store::{WalletStore, GENESIS_INDEX, FAUCET_INDEX};

/// A single planned output: the wallet-local index that owns it, its commitment,
/// and its plaintext value (known only to us - never appears on chain).
pub type PlannedOutput = (u32, Commitment, u64);

#[derive(Debug)]
pub struct SendPlan {
    pub transaction: Transaction,
    pub dest: PlannedOutput,
    pub change: Option<PlannedOutput>,
    /// Commitments of the inputs this plan spends, so the caller can mark them
    /// spent in its WalletStore only after a successful broadcast.
    pub spent_commitments: Vec<Commitment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    InsufficientBalance { have: u64, need: u64 },
}

/// The real treasury secret, supplied at runtime rather than committed to
/// source (see core::genesis's module doc comment) - required to spend the
/// devnet faucet's treasury-funded balance. Returns None if unset or
/// malformed rather than panicking: a node with no faucet secret configured
/// should just run with the faucet disabled, not crash its entire API
/// server over a feature it may not even want to offer (see
/// api::faucet::FaucetState::new).
pub(crate) fn treasury_blinding_from_env() -> Option<Scalar> {
    let hex = std::env::var("HAZE_TREASURY_BLINDING").ok()?;
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(Scalar::from_bits(bytes))
}

/// The blinding factor for a wallet-owned output index. The well-known genesis
/// output (see wallet/cli.rs's --claim-genesis flow) uses a fixed devnet secret
/// rather than a keystore-derived one.
pub(crate) fn blinding_for(keystore: &Keystore, index: u32) -> Scalar {
    if index == GENESIS_INDEX {
        Scalar::from(42u64)
    } else if index == FAUCET_INDEX {
        // Only ever reached if FaucetState successfully initialized with a
        // real secret in the first place (see FaucetState::new) - a
        // FAUCET_INDEX output is never added to a wallet store otherwise.
        treasury_blinding_from_env().expect("FAUCET_INDEX output present but HAZE_TREASURY_BLINDING is unset - this should be unreachable")
    } else {
        keystore.derive_blinding(index)
    }
}

/// Greedily selects confirmed-or-pending, on-chain-verified outputs covering
/// `target`, shared by both self-pay (plan_send) and the interactive slate
/// flow (wallet/slate.rs). Includes the wallet's own still-unconfirmed
/// outputs (change from a prior send, or an incoming payment not yet mined)
/// so a second send doesn't have to wait for the first to confirm - see
/// WalletStore::spendable_including_pending for why this is safe.
pub(crate) fn select_spendable(store: &WalletStore, target: u64) -> Result<Vec<PlannedOutput>, PlanError> {
    let mut selected: Vec<PlannedOutput> = Vec::new();
    let mut selected_total = 0u64;
    for output in store.spendable_including_pending() {
        if selected_total >= target {
            break;
        }
        selected.push((output.index, output.commitment, output.value));
        selected_total += output.value;
    }

    if selected_total < target {
        return Err(PlanError::InsufficientBalance { have: store.balance() + store.pending_balance(), need: target });
    }

    Ok(selected)
}

/// Builds a real, self-contained Mimblewimble transaction spending the wallet's own
/// confirmed UTXOs: a destination output of `amount`, an optional change output, and
/// a signed kernel. Pure and network-free - the caller is responsible for
/// broadcasting `transaction` and, only on success, applying `spent_commitments`/
/// `dest`/`change` to its WalletStore.
///
/// Allocates (and persists, via `keystore.allocate_index`) new output indices
/// eagerly, exactly as the desktop CLI does, so a crash never reuses a blinding
/// factor - even if the resulting transaction is never actually broadcast.
pub fn plan_send(keystore: &mut Keystore, store: &WalletStore, amount: u64, fee: u64) -> Result<SendPlan, PlanError> {
    let target = amount + fee;

    // 1. Greedily select confirmed, on-chain-verified outputs to cover amount + fee.
    let selected = select_spendable(store, target)?;
    let selected_total: u64 = selected.iter().map(|(_, _, value)| value).sum();

    // 2. Derive input blinding factors from the keystore.
    let mut input_blindings: Vec<Scalar> = Vec::new();
    let mut inputs: Vec<Input> = Vec::new();
    let mut spent_commitments: Vec<Commitment> = Vec::new();
    for (index, commitment, _value) in &selected {
        input_blindings.push(blinding_for(keystore, *index));
        inputs.push(Input { commitment: *commitment });
        spent_commitments.push(*commitment);
    }

    // 3. Allocate a destination output, and a change output if there's leftover.
    let note_key = keystore.note_key();
    let dest_index = keystore.allocate_index();
    let dest_blinding = keystore.derive_blinding(dest_index);
    let dest_commitment = Commitment::new(amount, dest_blinding);
    let dest_proof = RangeProof::prove(amount, &dest_blinding);
    let dest_note = note::seal(&note_key, dest_index, amount);
    let dest_output = Output { commitment: dest_commitment, proof: dest_proof, note: dest_note };

    let change_value = selected_total - target;
    let mut outputs = vec![dest_output];
    let mut output_blindings = vec![dest_blinding];

    let change = if change_value > 0 {
        let change_index = keystore.allocate_index();
        let change_blinding = keystore.derive_blinding(change_index);
        let change_commitment = Commitment::new(change_value, change_blinding);
        let change_proof = RangeProof::prove(change_value, &change_blinding);
        let change_note = note::seal(&note_key, change_index, change_value);
        outputs.push(Output { commitment: change_commitment, proof: change_proof, note: change_note });
        output_blindings.push(change_blinding);
        Some((change_index, change_commitment, change_value))
    } else {
        None
    };

    // 4. Compute the kernel excess and sign.
    let sum_input_blinding: Scalar = input_blindings.iter().sum();
    let sum_output_blinding: Scalar = output_blindings.iter().sum();
    let excess_blinding = sum_input_blinding - sum_output_blinding;
    let excess_commitment = Commitment::new(0, excess_blinding);
    let signature = Signature::sign(&fee.to_le_bytes(), &excess_blinding);

    let kernel = TxKernel {
        excess: excess_commitment,
        fee,
        signature,
    };

    let transaction = Transaction {
        inputs,
        outputs,
        kernels: vec![kernel],
    };

    Ok(SendPlan {
        transaction,
        dest: (dest_index, dest_commitment, amount),
        change,
        spent_commitments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::store::OutputStatus;

    fn seed_store_with_output(keystore: &mut Keystore, store: &mut WalletStore, value: u64) -> u32 {
        let index = keystore.allocate_index();
        let blinding = keystore.derive_blinding(index);
        let commitment = Commitment::new(value, blinding);
        store.add_output(index, value, commitment, OutputStatus::Confirmed);
        index
    }

    #[test]
    fn plan_send_produces_a_valid_transaction_with_change() {
        let mut keystore = Keystore::generate();
        let mut store = WalletStore::default();
        seed_store_with_output(&mut keystore, &mut store, 1000);

        let plan = plan_send(&mut keystore, &store, 100, 5).expect("plan should succeed");

        assert!(plan.transaction.validate(), "planned transaction must pass cryptographic validation");
        assert_eq!(plan.dest.2, 100);
        assert_eq!(plan.change.expect("expected change output").2, 1000 - 100 - 5);
        assert_eq!(plan.spent_commitments.len(), 1);
        assert_eq!(plan.transaction.outputs.len(), 2);
    }

    #[test]
    fn plan_send_with_exact_amount_has_no_change() {
        let mut keystore = Keystore::generate();
        let mut store = WalletStore::default();
        seed_store_with_output(&mut keystore, &mut store, 105);

        let plan = plan_send(&mut keystore, &store, 100, 5).expect("plan should succeed");

        assert!(plan.transaction.validate());
        assert!(plan.change.is_none());
        assert_eq!(plan.transaction.outputs.len(), 1);
    }

    #[test]
    fn plan_send_selects_multiple_inputs_when_needed() {
        let mut keystore = Keystore::generate();
        let mut store = WalletStore::default();
        seed_store_with_output(&mut keystore, &mut store, 60);
        seed_store_with_output(&mut keystore, &mut store, 60);

        let plan = plan_send(&mut keystore, &store, 100, 5).expect("plan should succeed");

        assert!(plan.transaction.validate());
        assert_eq!(plan.spent_commitments.len(), 2);
    }

    #[test]
    fn plan_send_rejects_insufficient_balance() {
        let mut keystore = Keystore::generate();
        let mut store = WalletStore::default();
        seed_store_with_output(&mut keystore, &mut store, 50);

        let err = plan_send(&mut keystore, &store, 100, 5).unwrap_err();

        assert_eq!(err, PlanError::InsufficientBalance { have: 50, need: 105 });
    }

    /// A wallet's own not-yet-mined change/incoming output must still be
    /// spendable - otherwise a second send has to wait for the first to
    /// confirm even though the funds are already, provably, the wallet's own
    /// (see WalletStore::spendable_including_pending for why this is safe).
    #[test]
    fn plan_send_can_spend_own_pending_outputs() {
        let mut keystore = Keystore::generate();
        let mut store = WalletStore::default();
        let index = keystore.allocate_index();
        let blinding = keystore.derive_blinding(index);
        let commitment = Commitment::new(1000, blinding);
        store.add_output(index, 1000, commitment, OutputStatus::Pending);

        let plan = plan_send(&mut keystore, &store, 100, 5).expect("plan should succeed using the pending output");

        assert!(plan.transaction.validate());
        assert_eq!(plan.spent_commitments, vec![commitment]);
    }

    /// The insufficient-balance error must report the true total available
    /// (confirmed + pending), not just the confirmed portion, now that both
    /// are eligible for spending.
    #[test]
    fn plan_send_insufficient_balance_error_sums_confirmed_and_pending() {
        let mut keystore = Keystore::generate();
        let mut store = WalletStore::default();
        seed_store_with_output(&mut keystore, &mut store, 20);
        let index = keystore.allocate_index();
        let blinding = keystore.derive_blinding(index);
        let commitment = Commitment::new(20, blinding);
        store.add_output(index, 20, commitment, OutputStatus::Pending);

        let err = plan_send(&mut keystore, &store, 100, 5).unwrap_err();

        assert_eq!(err, PlanError::InsufficientBalance { have: 40, need: 105 });
    }
}
