#[cfg(test)]
mod tests {
    use crate::core::transaction::{Transaction, Input, Output, TxKernel};
    use crate::core::cut_through::aggregate_and_cut_through;
    use crate::core::chain::ChainState;
    use crate::core::block::{Block, BlockHeader};
    use crate::crypto::pedersen::Commitment;
    use crate::crypto::range_proof::RangeProof;
    use crate::crypto::schnorr::Signature;
    
    use curve25519_dalek_ng::scalar::Scalar;
    use rand::rngs::OsRng;

    #[test]
    fn test_full_transaction_lifecycle() {
        let mut rng = OsRng;

        // --- TRANSACTION 1 ---
        // Inputs:
        //   In1: value 30, blinding r1
        //   In2: value 40, blinding r2
        // Outputs:
        //   Out1: value 25, blinding r3
        //   Out2: value 40, blinding r4
        // Fee: 5
        // Total Inputs (70) = Total Outputs (65) + Fee (5)
        
        let r1 = Scalar::random(&mut rng);
        let r2 = Scalar::random(&mut rng);
        let r3 = Scalar::random(&mut rng);
        let r4 = Scalar::random(&mut rng);

        let in1 = Input { commitment: Commitment::new(30, r1) };
        let in2 = Input { commitment: Commitment::new(40, r2) };

        println!("Generating range proof for Out1 (25)...");
        let proof1 = RangeProof::prove(25, &r3);
        let out1 = Output { commitment: Commitment::new(25, r3), proof: proof1 };

        println!("Generating range proof for Out2 (40)...");
        let proof2 = RangeProof::prove(40, &r4);
        let out2 = Output { commitment: Commitment::new(40, r4), proof: proof2 };

        let fee1 = 5u64;
        // Blinding excess equation: excess = sum(in_r) - sum(out_r)
        let excess_r1 = (r1 + r2) - (r3 + r4);
        let excess_commitment1 = Commitment::new(0, excess_r1);
        let signature1 = Signature::sign(&fee1.to_le_bytes(), &excess_r1);

        let kernel1 = TxKernel {
            excess: excess_commitment1,
            fee: fee1,
            signature: signature1,
        };

        let tx1 = Transaction {
            inputs: vec![in1.clone(), in2.clone()],
            outputs: vec![out1.clone(), out2.clone()],
            kernels: vec![kernel1.clone()],
        };

        // Assert Tx1 is cryptographically valid
        assert!(tx1.validate(), "Transaction 1 validation failed!");

        // --- TRANSACTION 2 ---
        // Spends Out1 from Tx1.
        // Inputs:
        //   In3: Out1 (value 25, blinding r3)
        // Outputs:
        //   Out3: value 20, blinding r5
        // Fee: 5
        // Total Inputs (25) = Total Outputs (20) + Fee (5)

        let r5 = Scalar::random(&mut rng);

        let in3 = Input { commitment: out1.commitment };

        println!("Generating range proof for Out3 (20)...");
        let proof3 = RangeProof::prove(20, &r5);
        let out3 = Output { commitment: Commitment::new(20, r5), proof: proof3 };

        let fee2 = 5u64;
        let excess_r2 = r3 - r5;
        let excess_commitment2 = Commitment::new(0, excess_r2);
        let signature2 = Signature::sign(&fee2.to_le_bytes(), &excess_r2);

        let kernel2 = TxKernel {
            excess: excess_commitment2,
            fee: fee2,
            signature: signature2,
        };

        let tx2 = Transaction {
            inputs: vec![in3.clone()],
            outputs: vec![out3.clone()],
            kernels: vec![kernel2.clone()],
        };

        // Assert Tx2 is cryptographically valid
        assert!(tx2.validate(), "Transaction 2 validation failed!");

        // --- BLOCK AGGREGATION & CUT-THROUGH ---
        // Combine Tx1 and Tx2 into a single transaction and apply cut-through.
        // The intermediate Output `out1` from Tx1 and Input `in3` from Tx2 should cancel out.
        
        let aggregated_tx = aggregate_and_cut_through(vec![tx1.clone(), tx2.clone()]);

        // Cut-through checks:
        // 1. Outputs should NOT contain Out1 (it was consumed).
        // 2. Inputs should NOT contain In3 (it spent Out1).
        // 3. Inputs should only be In1 and In2.
        // 4. Outputs should only be Out2 and Out3.
        assert_eq!(aggregated_tx.inputs.len(), 2);
        assert_eq!(aggregated_tx.outputs.len(), 2);
        assert_eq!(aggregated_tx.kernels.len(), 2);

        assert!(
            aggregated_tx.inputs.iter().any(|i| i.commitment == in1.commitment),
            "Aggregated inputs must contain In1"
        );
        assert!(
            aggregated_tx.inputs.iter().any(|i| i.commitment == in2.commitment),
            "Aggregated inputs must contain In2"
        );
        assert!(
            !aggregated_tx.inputs.iter().any(|i| i.commitment == in3.commitment),
            "Aggregated inputs must NOT contain In3 (cut-through failed)"
        );

        assert!(
            aggregated_tx.outputs.iter().any(|o| o.commitment == out2.commitment),
            "Aggregated outputs must contain Out2"
        );
        assert!(
            aggregated_tx.outputs.iter().any(|o| o.commitment == out3.commitment),
            "Aggregated outputs must contain Out3"
        );
        assert!(
            !aggregated_tx.outputs.iter().any(|o| o.commitment == out1.commitment),
            "Aggregated outputs must NOT contain Out1 (cut-through failed)"
        );

        // Assert that the aggregated transaction remains mathematically and cryptographically valid
        assert!(aggregated_tx.validate(), "Aggregated transaction validation failed!");

        // --- CHAIN STATE TRANSITION & GLOBAL INVARIANT CHECK ---
        let mut chain_state = ChainState::new();

        // Populate ChainState UTXOs with the initial inputs (as if they were already confirmed in history)
        chain_state.utxos.insert(in1.commitment);
        chain_state.utxos.insert(in2.commitment);

        // Construct a block with the aggregated transaction
        let block = Block {
            header: BlockHeader {
                height: 1,
                prev_hash: [0u8; 32],
                total_kernel_offset: Scalar::zero(),
                nonce: 0,
            },
            body: aggregated_tx,
        };

        // Apply the block to the chain state
        let applied = chain_state.apply_block(&block);
        assert!(applied, "Applying aggregated block to ChainState failed!");

        // Verify the unspent UTXO set in the chain state matches the expected outputs
        assert_eq!(chain_state.utxos.len(), 2);
        assert!(chain_state.utxos.contains(&out2.commitment), "UTXO set must contain Out2");
        assert!(chain_state.utxos.contains(&out3.commitment), "UTXO set must contain Out3");
        assert!(!chain_state.utxos.contains(&out1.commitment), "UTXO set must NOT contain Out1");
        assert!(!chain_state.utxos.contains(&in1.commitment), "UTXO set must NOT contain spent In1");
        assert!(!chain_state.utxos.contains(&in2.commitment), "UTXO set must NOT contain spent In2");

        // Verify global Mimblewimble balance invariant:
        // Sum(Initial UTXOs) - Sum(Final UTXOs) - Total Fee Commitment = Sum(Kernel Excesses)
        // Which is algebraically:
        // Sum(Initial UTXOs) - Sum(Final UTXOs) - Total Fee Commitment - Sum(Kernel Excesses) = 0
        let mut sum_initial = curve25519_dalek_ng::ristretto::RistrettoPoint::default();
        sum_initial += in1.commitment.as_point();
        sum_initial += in2.commitment.as_point();

        let mut sum_final = curve25519_dalek_ng::ristretto::RistrettoPoint::default();
        sum_final += out2.commitment.as_point();
        sum_final += out3.commitment.as_point();

        let total_fee = fee1 + fee2;
        let fee_commitment = Commitment::new(total_fee, Scalar::zero()).as_point();

        let mut sum_kernels = curve25519_dalek_ng::ristretto::RistrettoPoint::default();
        sum_kernels += kernel1.excess.as_point();
        sum_kernels += kernel2.excess.as_point();

        let expected_zero = sum_initial - sum_final - fee_commitment - sum_kernels;
        assert_eq!(
            expected_zero,
            curve25519_dalek_ng::ristretto::RistrettoPoint::default(),
            "Mimblewimble global balance invariant violated!"
        );

        println!("All lifecycle integration tests passed successfully!");
    }
}
