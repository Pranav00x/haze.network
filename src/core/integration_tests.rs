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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use std::sync::Arc;

    /// The registry root for a block that registers no names - matches what
    /// ChainState::apply_linear_block computes when name_ops is empty.
    fn empty_registry_root() -> [u8; 32] {
        crate::core::registry::compute_registry_root(&std::collections::HashMap::new())
    }

    /// Builds the ownership proof register_validator now requires in place
    /// of a raw blinding factor - see core::chain::stake_registration_message.
    fn stake_proof(commitment: &Commitment, value: u64, blinding: &Scalar) -> Signature {
        let msg = crate::core::chain::stake_registration_message(commitment, value);
        Signature::sign(&msg, blinding)
    }

    /// Regression test for a free memory-exhaustion vector: apply_block used
    /// to unconditionally insert EVERY received block into self.blocks
    /// (an uncapped, never-evicted HashMap) before any validation at all -
    /// anyone could flood a node's memory forever with trivially-cheap
    /// garbage blocks (no real signature or range proof required), and,
    /// combined with find_reorg_path's O(n^2) fork search, force expensive
    /// work while the global chain lock was held. Fixed by requiring
    /// block.validate() (self-contained - no chain state needed) to pass
    /// before ever storing a block.
    #[test]
    fn apply_block_never_stores_a_block_that_fails_standalone_validation() {
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        // A garbage block: an output with an absurd value, no kernel at all
        // to balance it, no real signature - trivially cheap to fabricate
        // in unlimited quantity.
        let garbage_header = BlockHeader {
            height: 1,
            prev_hash: genesis_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, Scalar::from(42u64)),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let garbage_body = Transaction {
            inputs: vec![],
            outputs: vec![Output {
                commitment: Commitment::new(999_999_999, Scalar::zero()),
                proof: RangeProof::prove(999_999_999, &Scalar::zero()),
                note: vec![],
            }],
            kernels: vec![],
        };
        let garbage_block = Block {
            header: garbage_header,
            body: garbage_body,
            name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![],
            launch_collection_ops: vec![], validator_ops: vec![],
        };
        let garbage_hash = garbage_block.header.hash();

        assert!(!chain_state.apply_block(&garbage_block).is_applied(), "a garbage block must be rejected");
        assert!(!chain_state.blocks.contains_key(&garbage_hash), "a block that fails standalone validation must never be stored at all");
        assert_eq!(chain_state.current_height, 0, "the garbage block must not have advanced the chain");
    }

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
        let out1 = Output { commitment: Commitment::new(25, r3), proof: proof1, note: vec![] };

        println!("Generating range proof for Out2 (40)...");
        let proof2 = RangeProof::prove(40, &r4);
        let out2 = Output { commitment: Commitment::new(40, r4), proof: proof2, note: vec![] };

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
        let out3 = Output { commitment: Commitment::new(20, r5), proof: proof3, note: vec![] };

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

        // Construct the coinbase transaction for block 1:
        // Value: block_reward_at(1) + total_fees (fee1 + fee2 = 5 + 5 = 10).
        let coinbase_value = crate::core::block::block_reward_at(1) + 10;
        let r_coinbase = Scalar::random(&mut rng);
        let coinbase_output = Output {
            commitment: Commitment::new(coinbase_value, r_coinbase),
            proof: RangeProof::prove(coinbase_value, &r_coinbase),
        note: vec![],
        };
        
        let coinbase_excess_r = Scalar::zero() - r_coinbase;
        let coinbase_kernel = TxKernel {
            excess: Commitment::new(0, coinbase_excess_r),
            fee: 0,
            signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
        };
        
        let mut block_body = aggregated_tx.clone();
        block_body.outputs.push(coinbase_output.clone());
        block_body.kernels.push(coinbase_kernel.clone());

        let private_key = Scalar::from(42u64);
        let mut header = BlockHeader {
            height: 1,
            prev_hash: [0u8; 32],
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        // Construct a block with the aggregated transaction (including coinbase)
        let block = Block {
            header,
            body: block_body,
            name_ops: vec![],
            transfer_ops: vec![],
            mint_ops: vec![],
            transfer_asset_ops: vec![],
            launch_collection_ops: vec![], validator_ops: vec![],
        };

        // Apply the block to the chain state
        let applied = chain_state.apply_block(&block).is_applied();
        assert!(applied, "Applying aggregated block to ChainState failed!");

        // Verify the unspent UTXO set in the chain state matches the expected outputs (Out2, Out3, and Coinbase Output)
        assert_eq!(chain_state.utxos.len(), 3);
        assert!(chain_state.utxos.contains(&out2.commitment), "UTXO set must contain Out2");
        assert!(chain_state.utxos.contains(&out3.commitment), "UTXO set must contain Out3");
        assert!(chain_state.utxos.contains(&coinbase_output.commitment), "UTXO set must contain Coinbase Output");
        assert!(!chain_state.utxos.contains(&out1.commitment), "UTXO set must NOT contain Out1");
        assert!(!chain_state.utxos.contains(&in1.commitment), "UTXO set must NOT contain spent In1");
        assert!(!chain_state.utxos.contains(&in2.commitment), "UTXO set must NOT contain spent In2");

        // Verify global Mimblewimble balance invariant:
        // Sum(Initial UTXOs) - Sum(Final UTXOs) - Total Fee Commitment + BLOCK_REWARD = Sum(Kernel Excesses)
        // Which is algebraically:
        // Sum(Initial UTXOs) - Sum(Final UTXOs) - Total Fee Commitment + BLOCK_REWARD - Sum(Kernel Excesses) = 0
        let mut sum_initial = curve25519_dalek_ng::ristretto::RistrettoPoint::default();
        sum_initial += in1.commitment.as_point();
        sum_initial += in2.commitment.as_point();

        let mut sum_final = curve25519_dalek_ng::ristretto::RistrettoPoint::default();
        sum_final += out2.commitment.as_point();
        sum_final += out3.commitment.as_point();
        sum_final += coinbase_output.commitment.as_point();

        let reward_commitment = Commitment::new(crate::core::block::block_reward_at(1), Scalar::zero()).as_point();

        let mut sum_kernels = curve25519_dalek_ng::ristretto::RistrettoPoint::default();
        sum_kernels += kernel1.excess.as_point();
        sum_kernels += kernel2.excess.as_point();
        sum_kernels += coinbase_kernel.excess.as_point();

        let expected_zero = sum_initial - sum_final + reward_commitment - sum_kernels;
        assert_eq!(
            expected_zero,
            curve25519_dalek_ng::ristretto::RistrettoPoint::default(),
            "Mimblewimble global balance invariant violated!"
        );

        println!("All lifecycle integration tests passed successfully!");
    }

    #[test]
    fn test_pos_selection() {
        let mut rng = OsRng;
        let mut chain_state = ChainState::new();

        // 1. Create and add validator inputs to UTXO set first
        let r_a = Scalar::random(&mut rng);
        let r_b = Scalar::random(&mut rng);
        let r_c = Scalar::random(&mut rng);

        let commitment_a = Commitment::new(1000, r_a);
        let commitment_b = Commitment::new(2000, r_b);
        let commitment_c = Commitment::new(3000, r_c);

        chain_state.utxos.insert(commitment_a);
        chain_state.utxos.insert(commitment_b);
        chain_state.utxos.insert(commitment_c);

        // 2. Register validators
        assert!(chain_state.register_validator(commitment_a, 1000, stake_proof(&commitment_a, 1000, &r_a)));
        assert!(chain_state.register_validator(commitment_b, 2000, stake_proof(&commitment_b, 2000, &r_b)));
        assert!(chain_state.register_validator(commitment_c, 3000, stake_proof(&commitment_c, 3000, &r_c)));

        assert_eq!(chain_state.active_validators.len(), 3);

        // 3. Selection must be deterministic
        let proposer1 = chain_state.select_proposer(1, [1u8; 32]);
        let proposer2 = chain_state.select_proposer(1, [1u8; 32]);
        assert_eq!(proposer1, proposer2);

        // 4. Over multiple slots, proposer distribution should select different validators
        let mut selected_a = 0;
        let mut selected_b = 0;
        let mut selected_c = 0;

        for h in 1u64..=100u64 {
            let mut prev_hash = [0u8; 32];
            prev_hash[0..8].copy_from_slice(&h.to_le_bytes());
            let proposer = chain_state.select_proposer(h, prev_hash);
            if proposer == commitment_a {
                selected_a += 1;
            } else if proposer == commitment_b {
                selected_b += 1;
            } else if proposer == commitment_c {
                selected_c += 1;
            }
        }

        println!("Selected counts - A: {}, B: {}, C: {}", selected_a, selected_b, selected_c);
        // Stakers with more weight should be selected more often
        assert!(selected_c > 0);
        assert!(selected_b > 0);
    }

    /// register_validator must not accept an unauthenticated value claim for
    /// someone else's real, unspent commitment - regression test for a bug
    /// where a peer-relayed validator entry was trusted without any proof,
    /// letting an attacker who merely observed a commitment on-chain (all
    /// commitments are public) claim an arbitrary stake weight for it.
    #[test]
    fn register_validator_rejects_a_forged_value_claim_for_a_real_commitment() {
        let mut rng = OsRng;
        let mut chain_state = ChainState::new();

        let r = Scalar::random(&mut rng);
        let real_value = 1_000u64;
        let commitment = Commitment::new(real_value, r);
        chain_state.utxos.insert(commitment);

        // A proof genuinely produced for a DIFFERENT value than the one
        // being claimed here must not validate - same signing message
        // would need to be re-signed for the new value, which requires the
        // real blinding factor the attacker doesn't have.
        let proof_for_real_value = stake_proof(&commitment, real_value, &r);
        assert!(!chain_state.register_validator(commitment, 999_999_999, proof_for_real_value));
        assert!(chain_state.active_validators.is_empty());

        // The correctly-signed (commitment, value) pair still works.
        let proof = stake_proof(&commitment, real_value, &r);
        assert!(chain_state.register_validator(commitment, real_value, proof));
        assert_eq!(chain_state.active_validators.len(), 1);
    }

    /// A validator's cached proof (as carried in ValidatorsList entries) is
    /// safe to re-submit/relay without ever touching the raw blinding
    /// factor - this is the whole point of proof-based registration rather
    /// than broadcasting the secret itself.
    #[test]
    fn register_validator_accepts_a_relayed_entrys_own_proof() {
        let mut rng = OsRng;
        let mut chain_a = ChainState::new();
        let mut chain_b = ChainState::new();

        let r = Scalar::random(&mut rng);
        let value = 4_000u64;
        let commitment = Commitment::new(value, r);
        chain_a.utxos.insert(commitment);
        chain_b.utxos.insert(commitment);

        let proof = stake_proof(&commitment, value, &r);
        assert!(chain_a.register_validator(commitment, value, proof.clone()));

        // Simulate relaying chain_a's resulting Validator entry (as
        // ValidatorsList would) to a second, independent node - it must be
        // accepted purely from the relayed (commitment, value, proof), with
        // no access to `r` at all.
        let relayed = chain_a.active_validators[0].clone();
        assert!(chain_b.register_validator(relayed.commitment, relayed.value, relayed.proof));
        assert_eq!(chain_b.active_validators.len(), 1);
    }

    /// proposer_priority_order must rank every active validator exactly
    /// once, agree with select_proposer at rank 0, and be deterministic -
    /// the property Proposer::start_proposing's fallback timing relies on.
    #[test]
    fn proposer_priority_order_ranks_every_validator_exactly_once() {
        let mut rng = OsRng;
        let mut chain_state = ChainState::new();

        let r_a = Scalar::random(&mut rng);
        let r_b = Scalar::random(&mut rng);
        let r_c = Scalar::random(&mut rng);
        let commitment_a = Commitment::new(1000, r_a);
        let commitment_b = Commitment::new(2000, r_b);
        let commitment_c = Commitment::new(3000, r_c);
        chain_state.utxos.insert(commitment_a);
        chain_state.utxos.insert(commitment_b);
        chain_state.utxos.insert(commitment_c);
        assert!(chain_state.register_validator(commitment_a, 1000, stake_proof(&commitment_a, 1000, &r_a)));
        assert!(chain_state.register_validator(commitment_b, 2000, stake_proof(&commitment_b, 2000, &r_b)));
        assert!(chain_state.register_validator(commitment_c, 3000, stake_proof(&commitment_c, 3000, &r_c)));

        let order1 = chain_state.proposer_priority_order(5, [9u8; 32]);
        let order2 = chain_state.proposer_priority_order(5, [9u8; 32]);
        assert_eq!(order1, order2, "priority order must be deterministic for the same (height, prev_hash)");
        assert_eq!(order1.len(), 3, "every active validator must appear exactly once");
        assert_eq!(order1[0], chain_state.select_proposer(5, [9u8; 32]), "rank 0 must match the historical single-winner select_proposer");

        let mut sorted = order1.clone();
        sorted.sort_by_key(|c| c.as_point().compress().to_bytes());
        let mut expected = vec![commitment_a, commitment_b, commitment_c];
        expected.sort_by_key(|c| c.as_point().compress().to_bytes());
        assert_eq!(sorted, expected, "priority order must contain exactly the active validator set, no duplicates or omissions");
    }

    /// The actual liveness fix: apply_linear_block must accept a block
    /// signed by ANY active, registered validator - not just the single
    /// computed select_proposer winner for that height. Before this fix,
    /// this exact scenario (an otherwise fully valid block from a
    /// non-primary validator) was rejected outright, which is what let a
    /// single offline majority-stake validator stall the whole chain
    /// forever (confirmed live in a 3-node test).
    #[test]
    fn apply_block_accepts_a_non_primary_active_validator_as_fallback_proposer() {
        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis = crate::core::genesis::genesis_block();
        assert!(chain_state.apply_block(&genesis).is_applied());

        // Register two validators with real (synthetic, test-only) backing
        // UTXOs - same pattern test_pos_selection already uses, since
        // register_validator only requires a real unspent commitment and
        // knowledge of its blinding, not that it came from genesis.
        let r_a = Scalar::random(&mut rng);
        let commitment_a = Commitment::new(500_000, r_a);
        chain_state.utxos.insert(commitment_a);
        assert!(chain_state.register_validator(commitment_a, 500_000, stake_proof(&commitment_a, 500_000, &r_a)));

        let r_b = Scalar::random(&mut rng);
        let commitment_b = Commitment::new(500_000, r_b);
        chain_state.utxos.insert(commitment_b);
        assert!(chain_state.register_validator(commitment_b, 500_000, stake_proof(&commitment_b, 500_000, &r_b)));

        let prev_hash = genesis.header.hash();
        let primary = chain_state.select_proposer(1, prev_hash);
        // Whichever validator ISN'T the computed primary for height 1 plays
        // the "fallback stepping in" role this test proves works.
        let (fallback_r, fallback_value) = if primary == commitment_a {
            (r_b, 500_000u64)
        } else {
            (r_a, 500_000u64)
        };

        let r_coinbase = Scalar::random(&mut rng);
        let reward = crate::core::block::block_reward_at(1);
        let coinbase_output = Output {
            commitment: Commitment::new(reward, r_coinbase),
            proof: RangeProof::prove(reward, &r_coinbase),
            note: vec![],
        };
        let coinbase_excess_r = Scalar::zero() - r_coinbase;
        let coinbase_kernel = TxKernel {
            excess: Commitment::new(0, coinbase_excess_r),
            fee: 0,
            signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
        };
        let body = Transaction { inputs: vec![], outputs: vec![coinbase_output], kernels: vec![coinbase_kernel] };

        let fallback_commitment = Commitment::new(fallback_value, fallback_r);
        assert_ne!(fallback_commitment, primary, "sanity: the fallback signer must genuinely differ from the computed primary");

        let mut header = BlockHeader {
            height: 1,
            prev_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: fallback_commitment,
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &fallback_r);
        let block = Block { header, body, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(chain_state.apply_block(&block).is_applied(), "a block signed by a non-primary but genuinely active validator must be accepted");
    }

    #[tokio::test]
    async fn test_dandelion_fluff_timeout() {
        use crate::p2p::dandelion::DandelionRouter;

        let router = DandelionRouter::new(0.0); // 0% fluff probability
        let tx_id = [7u8; 32];
        let fluffed = Arc::new(AtomicBool::new(false));
        
        let fluffed_clone = Arc::clone(&fluffed);
        router.register_stem_tx(tx_id, 1, move || {
            fluffed_clone.store(true, Ordering::SeqCst);
        });

        // Initially should not be fluffed
        assert!(!fluffed.load(Ordering::SeqCst));

        // Sleep to let timer expire (1s timeout)
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // Should now be fluffed via fallback trigger
        assert!(fluffed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_p2p_validator_propagation() {
        use tokio::io::AsyncWriteExt;

        let mut rng = OsRng;
        let mempool = Arc::new(std::sync::Mutex::new(crate::core::mempool::Mempool::new()));
        let chain_state = Arc::new(std::sync::Mutex::new(ChainState::new()));
        
        // 1. Create a validator commitment and add it to the UTXO set
        let r = Scalar::random(&mut rng);
        let value = 2500u64;
        let commitment = Commitment::new(value, r);
        {
            let mut c = chain_state.lock().unwrap();
            c.utxos.insert(commitment);
        }

        // 2. Start the P2pServer
        let test_db_path = format!("{}/haze_test_db_{}", std::env::temp_dir().display(), std::process::id());
        let storage = Arc::new(crate::core::storage::Storage::open_at(&test_db_path));
        let marketplace_state = Arc::new(crate::core::marketplace::MarketplaceState::new());
        let allowlist_state = Arc::new(crate::core::allowlist::AllowlistState::new());
        let p2p_server = Arc::new(crate::p2p::server::P2pServer::new(Arc::clone(&mempool), Arc::clone(&chain_state), storage, marketplace_state, allowlist_state));
        let server_clone = Arc::clone(&p2p_server);
        
        // Find a random free port and bind
        let bind_addr = "127.0.0.1:28333";
        tokio::spawn(async move {
            let _ = server_clone.start(bind_addr, vec![]).await;
        });

        // Sleep to let P2P server start listening
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 3. Connect as a client TcpStream
        let mut client_stream = tokio::net::TcpStream::connect(bind_addr).await.unwrap();

        // 4. Send Handshake
        let handshake = crate::p2p::message::P2pMessage::Handshake {
            listen_addr: "127.0.0.1:28334".to_string(),
        };
        let bytes = bincode::serialize(&handshake).unwrap();
        let len = bytes.len() as u32;
        client_stream.write_all(&len.to_le_bytes()).await.unwrap();
        client_stream.write_all(&bytes).await.unwrap();
        client_stream.flush().await.unwrap();

        // 5. Send NewValidatorOp - stake registration is now queued into the
        // mempool (like every other op) rather than mutating chain state
        // directly, so it only becomes an active validator once mined into
        // a block; this test verifies the P2P propagation/queueing step.
        let op = crate::core::chain::RegisterValidatorOp {
            commitment,
            value,
            proof: stake_proof(&commitment, value, &r),
        };
        let reg_msg = crate::p2p::message::P2pMessage::NewValidatorOp(op);
        let bytes = bincode::serialize(&reg_msg).unwrap();
        let len = bytes.len() as u32;
        client_stream.write_all(&len.to_le_bytes()).await.unwrap();
        client_stream.write_all(&bytes).await.unwrap();
        client_stream.flush().await.unwrap();

        // Sleep to let server handle message
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 6. Verify that the stake registration was queued into the mempool!
        let mut mp = mempool.lock().unwrap();
        let queued = mp.take_validator_ops();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].commitment, commitment);
        assert_eq!(queued[0].value, value);
    }

    #[test]
    fn test_chain_reorganization() {
        fn create_empty_block(height: u64, prev_hash: [u8; 32]) -> Block {
            let mut rng = OsRng;
            let private_key = Scalar::from(42u64);
            let r_coinbase = Scalar::random(&mut rng);
            let reward = crate::core::block::block_reward_at(height);

            let coinbase_output = Output {
                commitment: Commitment::new(reward, r_coinbase),
                proof: RangeProof::prove(reward, &r_coinbase),
            note: vec![],
            };
            let coinbase_excess_r = Scalar::zero() - r_coinbase;
            let coinbase_kernel = TxKernel {
                excess: Commitment::new(0, coinbase_excess_r),
                fee: 0,
                signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
            };
            
            let body = Transaction {
                inputs: vec![],
                outputs: vec![coinbase_output],
                kernels: vec![coinbase_kernel],
            };

            let mut header = BlockHeader {
                height,
                prev_hash,
                total_kernel_offset: Scalar::zero(),
                nonce: 0,
                timestamp: 0,
                validator_commitment: Commitment::new(1_000_000, private_key),
                validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
                name_registry_root: empty_registry_root(),
                chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
            };
            let msg = header.hash();
            header.validator_signature = Signature::sign(&msg, &private_key);

            Block { header, body, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] }
        }

        let mut chain_state = ChainState::new();

        // 1. Apply Genesis Block (height 0)
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());
        assert_eq!(chain_state.current_height, 0);

        // 2. Build and apply Block A1
        let a1 = create_empty_block(1, genesis_hash);
        let a1_hash = a1.header.hash();
        assert!(chain_state.apply_block(&a1).is_applied());
        assert_eq!(chain_state.current_height, 1);
        assert_eq!(chain_state.last_block_hash, a1_hash);

        // 3. Build and apply Block A2 (Main chaintip is now at height 2)
        let a2 = create_empty_block(2, a1_hash);
        let a2_hash = a2.header.hash();
        assert!(chain_state.apply_block(&a2).is_applied());
        assert_eq!(chain_state.current_height, 2);
        assert_eq!(chain_state.last_block_hash, a2_hash);

        // 4. Build competing fork from A1:
        // Block B2 (height 2, prev_hash = A1)
        let b2 = create_empty_block(2, a1_hash);
        let b2_hash = b2.header.hash();

        // Applying B2 should NOT change active tip (height 2 fork is same length as A2)
        assert!(!chain_state.apply_block(&b2).is_applied()); // Rejected because tip didn't switch
        assert_eq!(chain_state.current_height, 2);
        assert_eq!(chain_state.last_block_hash, a2_hash);

        // 5. Build Block B3 on top of B2 (height 3, prev_hash = B2)
        let b3 = create_empty_block(3, b2_hash);
        let b3_hash = b3.header.hash();

        // Applying B3 should trigger reorganization (height 3 > height 2)
        assert!(chain_state.apply_block(&b3).is_applied()); // Reorg applied because tip switched
        assert_eq!(chain_state.current_height, 3);
        assert_eq!(chain_state.last_block_hash, b3_hash);

        // Verify that A2 is no longer tip, and B2 and B3 are active
        assert!(chain_state.blocks.contains_key(&a2_hash));
        assert!(chain_state.blocks.contains_key(&b2_hash));
        assert!(chain_state.blocks.contains_key(&b3_hash));
    }

    #[test]
    fn test_name_registration_applies_and_rolls_back() {
        use crate::core::registry::{RegisterNameOp, NAME_REGISTRATION_FEE, NameRecord, compute_registry_root};
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        // Fund the registrant with a real UTXO: value 10, spending 5 as the
        // registration fee and getting 5 back as change.
        let r_in = Scalar::random(&mut rng);
        let r_change = Scalar::random(&mut rng);
        let input_commitment = Commitment::new(10, r_in);
        chain_state.utxos.insert(input_commitment);

        let change_output = Output {
            commitment: Commitment::new(5, r_change),
            proof: RangeProof::prove(5, &r_change),
        note: vec![],
        };
        let excess_r = r_in - r_change;
        let fee_payment = Transaction {
            inputs: vec![Input { commitment: input_commitment }],
            outputs: vec![change_output.clone()],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, excess_r),
                fee: NAME_REGISTRATION_FEE,
                signature: Signature::sign(&NAME_REGISTRATION_FEE.to_le_bytes(), &excess_r),
            }],
        };
        assert!(fee_payment.validate(), "fee_payment must be a valid standalone transaction");

        let gens = PedersenGens::default();
        let owner_secret = Scalar::random(&mut rng);
        let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
        let signature = RegisterNameOp::sign("pranav", &owner_secret);

        let op = RegisterNameOp {
            name: "pranav".to_string(),
            owner_pubkey,
            resolves_to: owner_pubkey,
            fee_payment,
            signature,
        };
        assert!(op.validate_standalone().is_ok());

        let mut expected_registry = std::collections::HashMap::new();
        expected_registry.insert(op.name.clone(), NameRecord {
            name: op.name.clone(),
            owner_pubkey: op.owner_pubkey,
            resolves_to: op.resolves_to,
            registered_at_block: 1,
        });
        let name_registry_root = compute_registry_root(&expected_registry);

        let private_key = Scalar::from(42u64);
        let coinbase_r = Scalar::random(&mut rng);
        let reward = crate::core::block::block_reward_at(1);
        let coinbase_output = Output {
            commitment: Commitment::new(reward, coinbase_r),
            proof: RangeProof::prove(reward, &coinbase_r),
        note: vec![],
        };
        let coinbase_excess_r = Scalar::zero() - coinbase_r;
        let body = Transaction {
            inputs: vec![],
            outputs: vec![coinbase_output],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, coinbase_excess_r),
                fee: 0,
                signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
            }],
        };

        let mut header = BlockHeader {
            height: 1,
            prev_hash: genesis_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root,
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        let block = Block { header, body, name_ops: vec![op], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(chain_state.apply_block(&block).is_applied(), "block with name registration must apply");
        assert!(chain_state.name_registry.contains_key("pranav"));
        assert_eq!(chain_state.name_registry["pranav"].resolves_to, owner_pubkey);
        assert!(!chain_state.utxos.contains(&input_commitment), "fee-payment input must be spent");
        assert!(chain_state.utxos.contains(&change_output.commitment), "fee-payment change must be in the UTXO set");

        assert!(chain_state.rollback_block().is_some());
        assert!(!chain_state.name_registry.contains_key("pranav"), "rollback must un-register the name");
        assert!(chain_state.utxos.contains(&input_commitment), "rollback must restore the spent input");
        assert!(!chain_state.utxos.contains(&change_output.commitment), "rollback must remove the change output");
    }

    /// A stake registration only takes effect once mined into a block (see
    /// core::chain::RegisterValidatorOp) - this is the actual determinism
    /// fix: every node derives active_validators purely from block content,
    /// not from whatever order a live RegisterValidator P2P message arrived
    /// in (the prior design). Also proves rollback correctly un-registers.
    #[test]
    fn block_with_validator_op_registers_and_rolls_back() {
        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        let r_stake = Scalar::random(&mut rng);
        let stake_value = 100_000u64;
        let stake_commitment = Commitment::new(stake_value, r_stake);
        chain_state.utxos.insert(stake_commitment);

        let validator_op = crate::core::chain::RegisterValidatorOp {
            commitment: stake_commitment,
            value: stake_value,
            proof: stake_proof(&stake_commitment, stake_value, &r_stake),
        };

        let private_key = Scalar::from(42u64);
        let coinbase_r = Scalar::random(&mut rng);
        let reward = crate::core::block::block_reward_at(1);
        let coinbase_output = Output {
            commitment: Commitment::new(reward, coinbase_r),
            proof: RangeProof::prove(reward, &coinbase_r),
            note: vec![],
        };
        let coinbase_excess_r = Scalar::zero() - coinbase_r;
        let body = Transaction {
            inputs: vec![],
            outputs: vec![coinbase_output],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, coinbase_excess_r),
                fee: 0,
                signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
            }],
        };

        let mut header = BlockHeader {
            height: 1,
            prev_hash: genesis_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        let block = Block { header, body, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![validator_op] };

        assert!(chain_state.apply_block(&block).is_applied(), "block with a valid stake registration must apply");
        assert_eq!(chain_state.active_validators.len(), 1);
        assert_eq!(chain_state.active_validators[0].commitment, stake_commitment);
        assert_eq!(chain_state.active_validators[0].value, stake_value);

        assert!(chain_state.rollback_block().is_some());
        assert!(chain_state.active_validators.is_empty(), "rollback must un-register the validator");
    }

    /// The atomicity property that makes this deterministic: a block
    /// containing an INVALID stake registration (forged value here) must be
    /// rejected in its entirety, not partially applied - same as every
    /// other op category (mirrors test_duplicate_name_registration_in_same_
    /// block_is_rejected's reasoning, applied to validator_ops).
    #[test]
    fn block_with_forged_validator_op_is_rejected_entirely() {
        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        let r_stake = Scalar::random(&mut rng);
        let stake_value = 100_000u64;
        let stake_commitment = Commitment::new(stake_value, r_stake);
        chain_state.utxos.insert(stake_commitment);

        // Proof genuinely signed for stake_value, but the op claims a wildly
        // different (forged) value - signature won't verify against it.
        let forged_op = crate::core::chain::RegisterValidatorOp {
            commitment: stake_commitment,
            value: 999_999_999,
            proof: stake_proof(&stake_commitment, stake_value, &r_stake),
        };

        let private_key = Scalar::from(42u64);
        let coinbase_r = Scalar::random(&mut rng);
        let reward = crate::core::block::block_reward_at(1);
        let coinbase_output = Output {
            commitment: Commitment::new(reward, coinbase_r),
            proof: RangeProof::prove(reward, &coinbase_r),
            note: vec![],
        };
        let coinbase_excess_r = Scalar::zero() - coinbase_r;
        let body = Transaction {
            inputs: vec![],
            outputs: vec![coinbase_output],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, coinbase_excess_r),
                fee: 0,
                signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
            }],
        };

        let mut header = BlockHeader {
            height: 1,
            prev_hash: genesis_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        let block = Block { header, body, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![forged_op] };

        assert!(!chain_state.apply_block(&block).is_applied(), "a block with a forged stake registration must be rejected entirely");
        assert_eq!(chain_state.current_height, 0, "rejected block must not advance the chain at all");
        assert!(chain_state.active_validators.is_empty());
    }

    #[test]
    fn test_duplicate_name_registration_in_same_block_is_rejected() {
        use crate::core::registry::{RegisterNameOp, NAME_REGISTRATION_FEE, NameRecord, compute_registry_root};
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        let gens = PedersenGens::default();

        let make_op = |rng: &mut OsRng, chain_state: &mut ChainState| -> RegisterNameOp {
            let r_in = Scalar::random(rng);
            let input_commitment = Commitment::new(NAME_REGISTRATION_FEE, r_in);
            chain_state.utxos.insert(input_commitment);
            let fee_payment = Transaction {
                inputs: vec![Input { commitment: input_commitment }],
                outputs: vec![],
                kernels: vec![TxKernel {
                    excess: Commitment::new(0, r_in),
                    fee: NAME_REGISTRATION_FEE,
                    signature: Signature::sign(&NAME_REGISTRATION_FEE.to_le_bytes(), &r_in),
                }],
            };
            let owner_secret = Scalar::random(rng);
            let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
            RegisterNameOp {
                name: "contested".to_string(),
                owner_pubkey,
                resolves_to: owner_pubkey,
                signature: RegisterNameOp::sign("contested", &owner_secret),
                fee_payment,
            }
        };

        let op1 = make_op(&mut rng, &mut chain_state);
        let op2 = make_op(&mut rng, &mut chain_state);

        let mut registry = std::collections::HashMap::new();
        registry.insert("contested".to_string(), NameRecord {
            name: "contested".to_string(),
            owner_pubkey: op1.owner_pubkey,
            resolves_to: op1.resolves_to,
            registered_at_block: 1,
        });

        let private_key = Scalar::from(42u64);
        let coinbase_r = Scalar::random(&mut rng);
        let reward = crate::core::block::block_reward_at(1);
        let coinbase_output = Output {
            commitment: Commitment::new(reward, coinbase_r),
            proof: RangeProof::prove(reward, &coinbase_r),
        note: vec![],
        };
        let coinbase_excess_r = Scalar::zero() - coinbase_r;
        let body = Transaction {
            inputs: vec![],
            outputs: vec![coinbase_output],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, coinbase_excess_r),
                fee: 0,
                signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
            }],
        };
        let mut header = BlockHeader {
            height: 1,
            prev_hash: genesis_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: compute_registry_root(&registry),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        let block = Block { header, body, name_ops: vec![op1, op2], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(!chain_state.apply_block(&block).is_applied(), "block registering the same name twice must be rejected");
    }

    fn build_coinbase_only_body(rng: &mut OsRng, height: u64) -> (Transaction, Scalar) {
        let coinbase_r = Scalar::random(rng);
        let reward = crate::core::block::block_reward_at(height);
        let coinbase_output = Output {
            commitment: Commitment::new(reward, coinbase_r),
            proof: RangeProof::prove(reward, &coinbase_r),
        note: vec![],
        };
        let coinbase_excess_r = Scalar::zero() - coinbase_r;
        let body = Transaction {
            inputs: vec![],
            outputs: vec![coinbase_output],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, coinbase_excess_r),
                fee: 0,
                signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
            }],
        };
        (body, coinbase_r)
    }

    #[test]
    fn test_name_transfer_applies_and_rolls_back() {
        use crate::core::registry::{RegisterNameOp, TransferNameOp, NAME_REGISTRATION_FEE, NameRecord, compute_registry_root};
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        let gens = PedersenGens::default();
        let private_key = Scalar::from(42u64);

        // --- Block 1: register "pranav" ---
        let r_in = Scalar::random(&mut rng);
        let input_commitment = Commitment::new(NAME_REGISTRATION_FEE, r_in);
        chain_state.utxos.insert(input_commitment);
        let fee_payment = Transaction {
            inputs: vec![Input { commitment: input_commitment }],
            outputs: vec![],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, r_in),
                fee: NAME_REGISTRATION_FEE,
                signature: Signature::sign(&NAME_REGISTRATION_FEE.to_le_bytes(), &r_in),
            }],
        };
        let original_secret = Scalar::random(&mut rng);
        let original_pubkey = Commitment(original_secret * gens.B_blinding);
        let register_op = RegisterNameOp {
            name: "pranav".to_string(),
            owner_pubkey: original_pubkey,
            resolves_to: original_pubkey,
            signature: RegisterNameOp::sign("pranav", &original_secret),
            fee_payment,
        };

        let mut registry_after_block1 = std::collections::HashMap::new();
        registry_after_block1.insert("pranav".to_string(), NameRecord {
            name: "pranav".to_string(),
            owner_pubkey: original_pubkey,
            resolves_to: original_pubkey,
            registered_at_block: 1,
        });

        let (body1, _) = build_coinbase_only_body(&mut rng, 1);
        let mut header1 = BlockHeader {
            height: 1,
            prev_hash: genesis_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: compute_registry_root(&registry_after_block1),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        header1.validator_signature = Signature::sign(&header1.hash(), &private_key);
        let block1 = Block { header: header1, body: body1, name_ops: vec![register_op], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block1).is_applied());
        let block1_hash = chain_state.last_block_hash;

        // --- Block 2: transfer "pranav" to a new owner, signed by the original owner ---
        let new_secret = Scalar::random(&mut rng);
        let new_pubkey = Commitment(new_secret * gens.B_blinding);
        let transfer_signature = TransferNameOp::sign("pranav", &new_pubkey, &new_pubkey, &original_secret);
        let transfer_op = TransferNameOp {
            name: "pranav".to_string(),
            new_owner_pubkey: new_pubkey,
            new_resolves_to: new_pubkey,
            signature: transfer_signature,
        };

        let mut registry_after_block2 = registry_after_block1.clone();
        registry_after_block2.insert("pranav".to_string(), NameRecord {
            name: "pranav".to_string(),
            owner_pubkey: new_pubkey,
            resolves_to: new_pubkey,
            registered_at_block: 1, // unchanged - original registration height
        });

        let (body2, _) = build_coinbase_only_body(&mut rng, 2);
        let mut header2 = BlockHeader {
            height: 2,
            prev_hash: block1_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: compute_registry_root(&registry_after_block2),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        header2.validator_signature = Signature::sign(&header2.hash(), &private_key);
        let block2 = Block { header: header2, body: body2, name_ops: vec![], transfer_ops: vec![transfer_op], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(chain_state.apply_block(&block2).is_applied(), "valid transfer must apply");
        assert_eq!(chain_state.name_registry["pranav"].owner_pubkey, new_pubkey);
        assert_eq!(chain_state.name_registry["pranav"].registered_at_block, 1, "transfer must not reset the original registration height");

        assert!(chain_state.rollback_block().is_some());
        assert_eq!(chain_state.name_registry["pranav"].owner_pubkey, original_pubkey, "rollback must restore the pre-transfer owner");
    }

    #[test]
    fn test_name_transfer_with_wrong_signer_is_rejected() {
        use crate::core::registry::{RegisterNameOp, TransferNameOp, NAME_REGISTRATION_FEE, NameRecord, compute_registry_root};
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        let gens = PedersenGens::default();
        let private_key = Scalar::from(42u64);

        let r_in = Scalar::random(&mut rng);
        let input_commitment = Commitment::new(NAME_REGISTRATION_FEE, r_in);
        chain_state.utxos.insert(input_commitment);
        let fee_payment = Transaction {
            inputs: vec![Input { commitment: input_commitment }],
            outputs: vec![],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, r_in),
                fee: NAME_REGISTRATION_FEE,
                signature: Signature::sign(&NAME_REGISTRATION_FEE.to_le_bytes(), &r_in),
            }],
        };
        let original_secret = Scalar::random(&mut rng);
        let original_pubkey = Commitment(original_secret * gens.B_blinding);
        let register_op = RegisterNameOp {
            name: "pranav".to_string(),
            owner_pubkey: original_pubkey,
            resolves_to: original_pubkey,
            signature: RegisterNameOp::sign("pranav", &original_secret),
            fee_payment,
        };
        let mut registry_after_block1 = std::collections::HashMap::new();
        registry_after_block1.insert("pranav".to_string(), NameRecord {
            name: "pranav".to_string(),
            owner_pubkey: original_pubkey,
            resolves_to: original_pubkey,
            registered_at_block: 1,
        });
        let (body1, _) = build_coinbase_only_body(&mut rng, 1);
        let mut header1 = BlockHeader {
            height: 1,
            prev_hash: genesis_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: compute_registry_root(&registry_after_block1),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        header1.validator_signature = Signature::sign(&header1.hash(), &private_key);
        let block1 = Block { header: header1, body: body1, name_ops: vec![register_op], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block1).is_applied());
        let block1_hash = chain_state.last_block_hash;

        // Attacker (not the real owner) tries to transfer "pranav" to themselves.
        let attacker_secret = Scalar::random(&mut rng);
        let attacker_pubkey = Commitment(attacker_secret * gens.B_blinding);
        let forged_signature = TransferNameOp::sign("pranav", &attacker_pubkey, &attacker_pubkey, &attacker_secret);
        let transfer_op = TransferNameOp {
            name: "pranav".to_string(),
            new_owner_pubkey: attacker_pubkey,
            new_resolves_to: attacker_pubkey,
            signature: forged_signature,
        };

        let mut registry_after_block2 = registry_after_block1.clone();
        registry_after_block2.insert("pranav".to_string(), NameRecord {
            name: "pranav".to_string(),
            owner_pubkey: attacker_pubkey,
            resolves_to: attacker_pubkey,
            registered_at_block: 1,
        });
        let (body2, _) = build_coinbase_only_body(&mut rng, 2);
        let mut header2 = BlockHeader {
            height: 2,
            prev_hash: block1_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: compute_registry_root(&registry_after_block2),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        header2.validator_signature = Signature::sign(&header2.hash(), &private_key);
        let block2 = Block { header: header2, body: body2, name_ops: vec![], transfer_ops: vec![transfer_op], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(!chain_state.apply_block(&block2).is_applied(), "transfer signed by a non-owner must be rejected");
        assert_eq!(chain_state.name_registry["pranav"].owner_pubkey, original_pubkey, "ownership must be unchanged");
    }

    /// Direct proof that RangeProof::verify() is bound to a specific
    /// commitment, not just "some valid-looking proof": a genuinely
    /// generated proof for (value=100, blinding=r) must fail against a
    /// different commitment, even one for the same value with a different
    /// blinding, or the same blinding with a different value.
    #[test]
    fn range_proof_verify_rejects_mismatched_commitment() {
        let mut rng = OsRng;
        let r = Scalar::random(&mut rng);
        let proof = RangeProof::prove(100, &r);
        let real_commitment = Commitment::new(100, r);
        assert!(proof.verify(&real_commitment), "a proof must verify against the exact commitment it was generated for");

        let different_value = Commitment::new(200, r);
        assert!(!proof.verify(&different_value), "a proof for value=100 must not verify against a value=200 commitment sharing the same blinding");

        let different_blinding = Commitment::new(100, Scalar::random(&mut rng));
        assert!(!proof.verify(&different_blinding), "a proof for blinding=r must not verify against the same value under a different blinding");
    }

    /// The concrete attack range proofs exist to prevent: curve25519's Scalar
    /// arithmetic is always mod the group order L, so a Pedersen commitment
    /// alone cannot distinguish "value = 100" from "value = L - k" (which
    /// behaves as "-k" in the balance equation, since k*H + (L-k)*H both
    /// reduce mod L). Without a range proof, an attacker could commit to
    /// value = -1000 on one output and +1100 on another, walk away with a
    /// genuinely spendable 1100-value output, and the naive point-arithmetic
    /// balance equation (sum_inputs - sum_outputs == kernel_excess) would
    /// still hold perfectly for a 100-value input - pure inflation.
    ///
    /// This builds exactly that transaction: real 100-value input, a
    /// "phantom" output whose raw commitment encodes value ≡ -1000 (built by
    /// hand via PedersenGens, bypassing Commitment::new's u64-only API,
    /// which cannot itself express an out-of-range value at all - already
    /// the first line of defense), paired with a genuinely spendable
    /// 1100-value output, with the kernel excess chosen so the balance
    /// equation checks out on paper. Since a valid Bulletproof cannot be
    /// generated for the phantom commitment (RangeProof::prove only accepts
    /// real u64 values), the best an attacker can attach is a validly-
    /// generated-but-unrelated proof - which the range-proof step must
    /// still catch, or the whole transaction validity check is broken.
    #[test]
    fn transaction_validate_rejects_wraparound_inflation_attack() {
        use bulletproofs::PedersenGens;
        let mut rng = OsRng;
        let gens = PedersenGens::default();

        // Real input: value 100.
        let r_in = Scalar::random(&mut rng);
        let input = Input { commitment: Commitment::new(100, r_in) };

        // Phantom output: raw commitment for value ≡ -1000 (mod L), i.e.
        // Scalar::zero() - Scalar::from(1000u64) - constructed by hand since
        // Commitment::new's u64 parameter can't even represent this value.
        let r_phantom = Scalar::random(&mut rng);
        let phantom_value_scalar = Scalar::zero() - Scalar::from(1000u64);
        let phantom_point = gens.commit(phantom_value_scalar, r_phantom);
        let phantom_commitment = Commitment(phantom_point);
        // The best an attacker can attach: a genuinely valid proof, just for
        // an unrelated value/commitment (proving an out-of-range value is
        // impossible via the typed API at all).
        let phantom_proof = RangeProof::prove(0, &Scalar::random(&mut rng));
        let phantom_output = Output { commitment: phantom_commitment, proof: phantom_proof, note: vec![] };

        // Real, honestly-proved output: value 1100 - what the attacker
        // actually walks away with if the range check doesn't catch this.
        let r_real = Scalar::random(&mut rng);
        let real_output = Output {
            commitment: Commitment::new(1100, r_real),
            proof: RangeProof::prove(1100, &r_real),
            note: vec![],
        };

        // Kernel excess chosen so sum_inputs - sum_outputs - fee == kernel_excess
        // holds exactly: 100*H - ((-1000+1100)*H) cancels the H component,
        // leaving only the blinding factors.
        let fee = 0u64;
        let excess_r = r_in - r_phantom - r_real;
        let kernel = TxKernel {
            excess: Commitment::new(0, excess_r),
            fee,
            signature: Signature::sign(&fee.to_le_bytes(), &excess_r),
        };

        let attack_tx = Transaction {
            inputs: vec![input],
            outputs: vec![phantom_output, real_output],
            kernels: vec![kernel],
        };

        assert!(!attack_tx.validate(), "a transaction with a wraparound/negative-implying output must be rejected");

        // Positive control: prove the rejection above is really about the
        // range proof, not some incidental mistake in this test's balance
        // arithmetic - the identical structure with an honest, in-range
        // value (900, matching the same balance equation without any
        // wraparound) must validate successfully.
        // Same 100-value input as the attack above, honestly split into two
        // outputs summing to exactly 100 (no phantom/wraparound value).
        let r_honest = Scalar::random(&mut rng);
        let honest_output = Output {
            commitment: Commitment::new(60, r_honest),
            proof: RangeProof::prove(60, &r_honest),
            note: vec![],
        };
        let r_real2 = Scalar::random(&mut rng);
        let real_output2 = Output {
            commitment: Commitment::new(40, r_real2),
            proof: RangeProof::prove(40, &r_real2),
            note: vec![],
        };
        let excess_r2 = r_in - r_honest - r_real2;
        let honest_tx = Transaction {
            inputs: vec![Input { commitment: Commitment::new(100, r_in) }],
            outputs: vec![honest_output, real_output2],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, excess_r2),
                fee,
                signature: Signature::sign(&fee.to_le_bytes(), &excess_r2),
            }],
        };
        assert!(honest_tx.validate(), "an honest, in-range, correctly-balanced transaction must still validate");
    }

    /// Confirms apply_linear_block (the single path every block - local,
    /// synced, forked, genesis - goes through) rejects the same attack at
    /// the full block/chain level, not just Transaction::validate() in
    /// isolation - proving there's no separate "fast path" during block
    /// application that skips range proof verification.
    #[test]
    fn apply_block_rejects_block_containing_wraparound_inflation_attack() {
        use bulletproofs::PedersenGens;
        let mut rng = OsRng;
        let gens = PedersenGens::default();
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        // Spend the real genesis validator-stake output (1_000_000, blinding=42).
        let r_in = Scalar::from(42u64);
        let input = Input { commitment: Commitment::new(1_000_000, r_in) };

        let r_phantom = Scalar::random(&mut rng);
        let phantom_value_scalar = Scalar::zero() - Scalar::from(500_000u64);
        let phantom_commitment = Commitment(gens.commit(phantom_value_scalar, r_phantom));
        let phantom_proof = RangeProof::prove(0, &Scalar::random(&mut rng));
        let phantom_output = Output { commitment: phantom_commitment, proof: phantom_proof, note: vec![] };

        let r_real = Scalar::random(&mut rng);
        let real_value = 1_000_000 + crate::core::block::block_reward_at(1) + 500_000; // balances against reward + phantom
        let real_output = Output {
            commitment: Commitment::new(real_value, r_real),
            proof: RangeProof::prove(real_value, &r_real),
            note: vec![],
        };

        let fee = 0u64;
        let excess_r = r_in - r_phantom - r_real;
        let kernel = TxKernel {
            excess: Commitment::new(0, excess_r),
            fee,
            signature: Signature::sign(&fee.to_le_bytes(), &excess_r),
        };

        let body = Transaction { inputs: vec![input], outputs: vec![phantom_output, real_output], kernels: vec![kernel] };
        assert!(!body.validate_with_reward(crate::core::block::block_reward_at(1)), "sanity: the body alone must already fail with the block reward applied");

        let private_key = Scalar::from(42u64);
        let mut header = BlockHeader {
            height: 1,
            prev_hash: genesis_block.header.hash(),
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);
        let block = Block { header, body, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(!chain_state.apply_block(&block).is_applied(), "a block containing a wraparound-inflated output must be rejected at apply time");
        assert_eq!(chain_state.current_height, 0, "chain must not have advanced past genesis");
    }

    /// Locks in the halving schedule's exact boundary behavior: full reward
    /// right up to (and including) the last block of an interval, halved
    /// reward the instant the next interval starts, and permanently zero
    /// once enough halvings have occurred to shift the initial reward to 0.
    #[test]
    fn block_reward_at_halves_on_schedule_boundaries() {
        use crate::core::block::block_reward_at;
        use crate::core::genesis::{HALVING_INTERVAL_BLOCKS, INITIAL_BLOCK_REWARD};

        assert_eq!(block_reward_at(0), INITIAL_BLOCK_REWARD);
        assert_eq!(block_reward_at(1), INITIAL_BLOCK_REWARD);
        assert_eq!(block_reward_at(HALVING_INTERVAL_BLOCKS - 1), INITIAL_BLOCK_REWARD);
        assert_eq!(block_reward_at(HALVING_INTERVAL_BLOCKS), INITIAL_BLOCK_REWARD / 2);
        assert_eq!(block_reward_at(HALVING_INTERVAL_BLOCKS * 2), INITIAL_BLOCK_REWARD / 4);

        // 540 halved 10 times reaches exactly 0 (540 -> 270 -> ... -> 1 -> 0).
        assert_eq!(block_reward_at(HALVING_INTERVAL_BLOCKS * 10), 0);
        assert_eq!(block_reward_at(HALVING_INTERVAL_BLOCKS * 100), 0, "reward must stay permanently at zero, never wrap or resume");
    }

    /// GENESIS_TOTAL_MINTED is a hand-maintained sum (validator stake + the
    /// four allocation constants) - this catches drift if any allocation
    /// constant is edited without updating the total, which would otherwise
    /// silently break every height-0 balance check.
    #[test]
    fn genesis_total_minted_matches_sum_of_genesis_outputs() {
        use crate::core::genesis::{genesis_block, GENESIS_TOTAL_MINTED};

        let genesis = genesis_block();

        // Outputs don't expose their plaintext value directly, so instead
        // confirm the block validates against the declared total - if any
        // allocation constant drifted out of sync with GENESIS_TOTAL_MINTED,
        // this balance-equation check would fail.
        assert!(genesis.body.validate_with_reward(GENESIS_TOTAL_MINTED), "genesis outputs must balance exactly against GENESIS_TOTAL_MINTED");
        assert!(!genesis.body.validate_with_reward(GENESIS_TOTAL_MINTED + 1), "genesis outputs must NOT balance against any other total - proves the check isn't vacuous");
        assert_eq!(genesis.body.outputs.len(), 17, "expected exactly 17 genesis outputs: validator stake + 7 team tranches + 7 investor tranches + airdrop + treasury");
    }

    /// A block whose chain_id doesn't match this network's must be rejected
    /// outright by apply_linear_block, even if every other field (reward,
    /// signature, range proofs) is perfectly valid - proving two networks can
    /// never accidentally interoperate over P2P.
    #[test]
    fn apply_block_rejects_mismatched_chain_id() {
        let mut chain_state = ChainState::new();
        let genesis = crate::core::genesis::genesis_block();
        assert!(chain_state.apply_block(&genesis).is_applied());

        let private_key = Scalar::from(42u64);
        let r_coinbase = Scalar::random(&mut OsRng);
        let reward = crate::core::block::block_reward_at(1);
        let coinbase_output = Output {
            commitment: Commitment::new(reward, r_coinbase),
            proof: RangeProof::prove(reward, &r_coinbase),
            note: vec![],
        };
        let coinbase_excess_r = Scalar::zero() - r_coinbase;
        let coinbase_kernel = TxKernel {
            excess: Commitment::new(0, coinbase_excess_r),
            fee: 0,
            signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
        };
        let body = Transaction { inputs: vec![], outputs: vec![coinbase_output], kernels: vec![coinbase_kernel] };

        let mut header = BlockHeader {
            height: 1,
            prev_hash: genesis.header.hash(),
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID.wrapping_add(1),
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);
        let block = Block { header, body, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(!chain_state.apply_block(&block).is_applied(), "a block with a mismatched chain_id must be rejected");
        assert_eq!(chain_state.current_height, 0, "chain must not have advanced past genesis");
    }

    // core::vesting's own tests (spends_locked_output_early_rejects_before_
    // unlock_and_allows_after, etc.) already prove the exact real genesis
    // team tranche commitment (via locked_genesis_outputs()[0], which reads
    // core::genesis::TEAM_TRANCHES[0].commitment() - the same fixed value
    // baked into the real chain) is correctly recognized as locked before
    // its cliff and unlocked after. A prior version of this file also had a
    // full apply_block-level test that built a *signed, otherwise-valid*
    // spend of that tranche to prove the block-level rejection wasn't an
    // incidental balance error - that test is gone because it's now
    // impossible to write: the blinding secret backing every locked
    // tranche is a real, randomly-generated value that was handed off
    // out-of-band and does not exist anywhere in this repo (see
    // core::genesis's module doc comment), so no test, anywhere, can ever
    // forge a valid signature for spending one. That's the fix working as
    // intended, not a coverage gap.

    /// Full ChainState-level proof for the new asset registry (see
    /// core::assets), mirroring test_name_registration_applies_and_rolls_back:
    /// a real mint applies (fee spent, registry updated, root checked) and a
    /// rollback restores everything exactly.
    #[test]
    fn test_asset_mint_applies_and_rolls_back() {
        use crate::core::assets::{MintAssetOp, AssetRecord, ASSET_MINT_FEE, compute_asset_registry_root};
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        // Fund the minter with a real UTXO: value 10, spending 5 as the mint
        // fee and getting 5 back as change.
        let r_in = Scalar::random(&mut rng);
        let r_change = Scalar::random(&mut rng);
        let input_commitment = Commitment::new(10, r_in);
        chain_state.utxos.insert(input_commitment);

        let change_output = Output {
            commitment: Commitment::new(5, r_change),
            proof: RangeProof::prove(5, &r_change),
        note: vec![],
        };
        let excess_r = r_in - r_change;
        let fee_payment = Transaction {
            inputs: vec![Input { commitment: input_commitment }],
            outputs: vec![change_output.clone()],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, excess_r),
                fee: ASSET_MINT_FEE,
                signature: Signature::sign(&ASSET_MINT_FEE.to_le_bytes(), &excess_r),
            }],
        };
        assert!(fee_payment.validate(), "fee_payment must be a valid standalone transaction");

        let gens = PedersenGens::default();
        let owner_secret = Scalar::random(&mut rng);
        let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
        let metadata = vec![7u8; 4];
        let signature = MintAssetOp::sign("cryptopunk", &metadata, &None, &None, &None, &owner_secret);

        let op = MintAssetOp {
            asset_id: "cryptopunk".to_string(),
            owner_pubkey,
            metadata,
            fee_payment,
            collection_id: None,
            phase_index: None,
            allowlist_proof: None,
            allowlist_leaf_index: None,
            required_kernel_excess: None,
            creator_signature: None,
            signature,
        };
        assert!(op.validate_standalone().is_ok());

        let mut expected_registry = std::collections::HashMap::new();
        expected_registry.insert(op.asset_id.clone(), AssetRecord {
            asset_id: op.asset_id.clone(),
            owner_pubkey: op.owner_pubkey,
            metadata: op.metadata.clone(),
            minted_at_block: 1,
            collection_id: None,
        });
        let asset_registry_root = compute_asset_registry_root(&expected_registry);

        let private_key = Scalar::from(42u64);
        let coinbase_r = Scalar::random(&mut rng);
        let reward = crate::core::block::block_reward_at(1);
        let coinbase_output = Output {
            commitment: Commitment::new(reward, coinbase_r),
            proof: RangeProof::prove(reward, &coinbase_r),
        note: vec![],
        };
        let coinbase_excess_r = Scalar::zero() - coinbase_r;
        let body = Transaction {
            inputs: vec![],
            outputs: vec![coinbase_output],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, coinbase_excess_r),
                fee: 0,
                signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
            }],
        };

        let mut header = BlockHeader {
            height: 1,
            prev_hash: genesis_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root,
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        let block = Block { header, body, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![op], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(chain_state.apply_block(&block).is_applied(), "block with an asset mint must apply");
        assert!(chain_state.asset_registry.contains_key("cryptopunk"));
        assert_eq!(chain_state.asset_registry["cryptopunk"].owner_pubkey, owner_pubkey);
        assert!(!chain_state.utxos.contains(&input_commitment), "mint fee-payment input must be spent");
        assert!(chain_state.utxos.contains(&change_output.commitment), "mint fee-payment change must be in the UTXO set");

        assert!(chain_state.rollback_block().is_some());
        assert!(!chain_state.asset_registry.contains_key("cryptopunk"), "rollback must un-mint the asset");
        assert!(chain_state.utxos.contains(&input_commitment), "rollback must restore the spent input");
        assert!(!chain_state.utxos.contains(&change_output.commitment), "rollback must remove the change output");
    }

    /// Mirrors test_duplicate_name_registration_in_same_block_is_rejected -
    /// two mints for the same asset_id in one block must be rejected.
    #[test]
    fn test_duplicate_asset_mint_in_same_block_is_rejected() {
        use crate::core::assets::{MintAssetOp, AssetRecord, compute_asset_registry_root};
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        let gens = PedersenGens::default();

        let make_op = |rng: &mut OsRng, chain_state: &mut ChainState| -> MintAssetOp {
            let r_in = Scalar::random(rng);
            let input_commitment = Commitment::new(crate::core::assets::ASSET_MINT_FEE, r_in);
            chain_state.utxos.insert(input_commitment);
            let fee_payment = Transaction {
                inputs: vec![Input { commitment: input_commitment }],
                outputs: vec![],
                kernels: vec![TxKernel {
                    excess: Commitment::new(0, r_in),
                    fee: crate::core::assets::ASSET_MINT_FEE,
                    signature: Signature::sign(&crate::core::assets::ASSET_MINT_FEE.to_le_bytes(), &r_in),
                }],
            };
            let owner_secret = Scalar::random(rng);
            let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
            let metadata = vec![4u8; 4];
            MintAssetOp {
                asset_id: "contested-punk".to_string(),
                owner_pubkey,
                metadata: metadata.clone(),
                signature: MintAssetOp::sign("contested-punk", &metadata, &None, &None, &None, &owner_secret),
                fee_payment,
                collection_id: None,
                phase_index: None,
                allowlist_proof: None,
                allowlist_leaf_index: None,
                required_kernel_excess: None,
                creator_signature: None,
            }
        };

        let op1 = make_op(&mut rng, &mut chain_state);
        let op2 = make_op(&mut rng, &mut chain_state);

        let mut registry = std::collections::HashMap::new();
        registry.insert("contested-punk".to_string(), AssetRecord {
            asset_id: "contested-punk".to_string(),
            owner_pubkey: op1.owner_pubkey,
            metadata: op1.metadata.clone(),
            minted_at_block: 1,
            collection_id: None,
        });

        let private_key = Scalar::from(42u64);
        let coinbase_r = Scalar::random(&mut rng);
        let reward = crate::core::block::block_reward_at(1);
        let coinbase_output = Output {
            commitment: Commitment::new(reward, coinbase_r),
            proof: RangeProof::prove(reward, &coinbase_r),
        note: vec![],
        };
        let coinbase_excess_r = Scalar::zero() - coinbase_r;
        let body = Transaction {
            inputs: vec![],
            outputs: vec![coinbase_output],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, coinbase_excess_r),
                fee: 0,
                signature: Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r),
            }],
        };
        let mut header = BlockHeader {
            height: 1,
            prev_hash: genesis_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&registry),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        let block = Block { header, body, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![op1, op2], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(!chain_state.apply_block(&block).is_applied(), "block minting the same asset_id twice must be rejected");
    }

    /// Builds a coinbase-balanced body (see build_coinbase_only_body) plus
    /// one extra, self-contained, zero-fee "payment": input value equals
    /// output value, so it contributes nothing extra to the reward balance
    /// equation beyond its own kernel excess - lets a test bundle a real,
    /// freely-referenceable kernel (standing in for a marketplace buyer's
    /// payment) alongside the coinbase without disturbing block_reward_at.
    /// Returns the body, the payment's spendable input commitment (the
    /// caller must insert this into chain_state.utxos before applying), and
    /// the payment kernel's excess (what a TransferAssetOp::required_kernel_excess
    /// would reference).
    fn build_body_with_payment(rng: &mut OsRng, height: u64, payer_input_value: u64) -> (Transaction, Commitment, Commitment) {
        let (mut body, _) = build_coinbase_only_body(rng, height);

        let r_in = Scalar::random(rng);
        let r_out = Scalar::random(rng);
        let input_commitment = Commitment::new(payer_input_value, r_in);
        let output_commitment = Commitment::new(payer_input_value, r_out);
        let output = Output { commitment: output_commitment, proof: RangeProof::prove(payer_input_value, &r_out), note: vec![] };
        let excess_r = r_in - r_out;
        let kernel = TxKernel {
            excess: Commitment::new(0, excess_r),
            fee: 0,
            signature: Signature::sign(&0u64.to_le_bytes(), &excess_r),
        };
        let payment_excess = kernel.excess;

        body.inputs.push(Input { commitment: input_commitment });
        body.outputs.push(output);
        body.kernels.push(kernel);

        (body, input_commitment, payment_excess)
    }

    /// Mints "marketswap-punk" in block 1 under `owner_secret`, returning the
    /// chain state (with genesis + block1 applied), the block1 hash, the
    /// owner's secret/pubkey, and a fresh buyer pubkey - shared setup for
    /// every required_kernel_excess test below.
    fn setup_chain_with_one_minted_asset(rng: &mut OsRng) -> (ChainState, [u8; 32], Scalar, Commitment, Commitment) {
        use crate::core::assets::{MintAssetOp, AssetRecord, ASSET_MINT_FEE, compute_asset_registry_root};
        use bulletproofs::PedersenGens;

        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        let gens = PedersenGens::default();
        let private_key = Scalar::from(42u64);

        let r_in = Scalar::random(rng);
        let input_commitment = Commitment::new(ASSET_MINT_FEE, r_in);
        chain_state.utxos.insert(input_commitment);
        let fee_payment = Transaction {
            inputs: vec![Input { commitment: input_commitment }],
            outputs: vec![],
            kernels: vec![TxKernel {
                excess: Commitment::new(0, r_in),
                fee: ASSET_MINT_FEE,
                signature: Signature::sign(&ASSET_MINT_FEE.to_le_bytes(), &r_in),
            }],
        };
        let owner_secret = Scalar::random(rng);
        let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
        let metadata = vec![1u8; 4];
        let mint_op = MintAssetOp {
            asset_id: "marketswap-punk".to_string(),
            owner_pubkey,
            metadata: metadata.clone(),
            fee_payment,
            signature: MintAssetOp::sign("marketswap-punk", &metadata, &None, &None, &None, &owner_secret),
            collection_id: None,
            phase_index: None,
            allowlist_proof: None,
            allowlist_leaf_index: None,
            required_kernel_excess: None,
            creator_signature: None,
        };

        let mut registry = std::collections::HashMap::new();
        registry.insert("marketswap-punk".to_string(), AssetRecord {
            asset_id: "marketswap-punk".to_string(),
            owner_pubkey,
            metadata,
            minted_at_block: 1,
            collection_id: None,
        });

        let (body1, _) = build_coinbase_only_body(rng, 1);
        let mut header1 = BlockHeader {
            height: 1,
            prev_hash: genesis_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&registry),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        header1.validator_signature = Signature::sign(&header1.hash(), &private_key);
        let block1 = Block { header: header1, body: body1, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![mint_op], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block1).is_applied(), "setup mint must apply");
        let block1_hash = chain_state.last_block_hash;

        let buyer_secret = Scalar::random(rng);
        let buyer_pubkey = Commitment(buyer_secret * gens.B_blinding);

        (chain_state, block1_hash, owner_secret, owner_pubkey, buyer_pubkey)
    }

    #[test]
    fn transfer_asset_op_with_unsatisfied_kernel_condition_is_rejected() {
        use crate::core::assets::{TransferAssetOp, compute_asset_registry_root};

        let mut rng = OsRng;
        let (mut chain_state, block1_hash, owner_secret, owner_pubkey, buyer_pubkey) = setup_chain_with_one_minted_asset(&mut rng);

        // A kernel excess that has never appeared on this chain.
        let phantom_excess = Commitment::new(0, Scalar::from(999_999u64));
        let transfer_op = TransferAssetOp {
            asset_id: "marketswap-punk".to_string(),
            new_owner_pubkey: buyer_pubkey,
            required_kernel_excess: Some(phantom_excess),
            required_royalty_kernel_excess: None,
            signature: TransferAssetOp::sign("marketswap-punk", &buyer_pubkey, &Some(phantom_excess), &None, &owner_secret),
        };

        let mut registry_after = std::collections::HashMap::new();
        registry_after.insert("marketswap-punk".to_string(), crate::core::assets::AssetRecord {
            asset_id: "marketswap-punk".to_string(),
            owner_pubkey: buyer_pubkey,
            metadata: vec![1u8; 4],
            minted_at_block: 1,
            collection_id: None,
        });

        let private_key = Scalar::from(42u64);
        let (body2, _) = build_coinbase_only_body(&mut rng, 2);
        let mut header2 = BlockHeader {
            height: 2,
            prev_hash: block1_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&registry_after),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        header2.validator_signature = Signature::sign(&header2.hash(), &private_key);
        let block2 = Block { header: header2, body: body2, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![transfer_op], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(!chain_state.apply_block(&block2).is_applied(), "a conditional transfer whose required kernel never landed must be rejected");
        assert_eq!(chain_state.asset_registry["marketswap-punk"].owner_pubkey, owner_pubkey, "ownership must not change");
    }

    #[test]
    fn transfer_asset_op_with_satisfied_historical_kernel_condition_is_accepted() {
        use crate::core::assets::{TransferAssetOp, compute_asset_registry_root};

        let mut rng = OsRng;
        let (mut chain_state, block1_hash, owner_secret, _owner_pubkey, buyer_pubkey) = setup_chain_with_one_minted_asset(&mut rng);
        let private_key = Scalar::from(42u64);

        // --- Block 2: the buyer's payment lands on its own, no transfer yet ---
        let (body2, payment_input, payment_excess) = build_body_with_payment(&mut rng, 2, 500);
        chain_state.utxos.insert(payment_input);
        let mut header2 = BlockHeader {
            height: 2,
            prev_hash: block1_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&{
                let mut r = std::collections::HashMap::new();
                r.insert("marketswap-punk".to_string(), chain_state.asset_registry["marketswap-punk"].clone());
                r
            }),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        header2.validator_signature = Signature::sign(&header2.hash(), &private_key);
        let block2 = Block { header: header2, body: body2, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block2).is_applied(), "payment-only block must apply");
        assert!(chain_state.kernel_excesses.contains(&payment_excess), "the payment kernel must now be indexed");
        let block2_hash = chain_state.last_block_hash;

        // --- Block 3: the conditional transfer, referencing block 2's now-historical kernel ---
        let transfer_op = TransferAssetOp {
            asset_id: "marketswap-punk".to_string(),
            new_owner_pubkey: buyer_pubkey,
            required_kernel_excess: Some(payment_excess),
            required_royalty_kernel_excess: None,
            signature: TransferAssetOp::sign("marketswap-punk", &buyer_pubkey, &Some(payment_excess), &None, &owner_secret),
        };
        let mut registry_after = std::collections::HashMap::new();
        registry_after.insert("marketswap-punk".to_string(), crate::core::assets::AssetRecord {
            asset_id: "marketswap-punk".to_string(),
            owner_pubkey: buyer_pubkey,
            metadata: vec![1u8; 4],
            minted_at_block: 1,
            collection_id: None,
        });
        let (body3, _) = build_coinbase_only_body(&mut rng, 3);
        let mut header3 = BlockHeader {
            height: 3,
            prev_hash: block2_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&registry_after),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        header3.validator_signature = Signature::sign(&header3.hash(), &private_key);
        let block3 = Block { header: header3, body: body3, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![transfer_op], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(chain_state.apply_block(&block3).is_applied(), "a conditional transfer referencing an already-historical kernel must apply");
        assert_eq!(chain_state.asset_registry["marketswap-punk"].owner_pubkey, buyer_pubkey);
    }

    #[test]
    fn transfer_asset_op_with_satisfied_same_block_kernel_condition_is_accepted() {
        use crate::core::assets::{TransferAssetOp, compute_asset_registry_root};

        let mut rng = OsRng;
        let (mut chain_state, block1_hash, owner_secret, _owner_pubkey, buyer_pubkey) = setup_chain_with_one_minted_asset(&mut rng);
        let private_key = Scalar::from(42u64);

        // Payment and its conditional transfer bundled into the SAME block -
        // the one-block atomic swap case. Without checking this block's own
        // kernels (not just historical ones), this would fail even though
        // the design intends it to work.
        let (body2, payment_input, payment_excess) = build_body_with_payment(&mut rng, 2, 500);
        chain_state.utxos.insert(payment_input);

        let transfer_op = TransferAssetOp {
            asset_id: "marketswap-punk".to_string(),
            new_owner_pubkey: buyer_pubkey,
            required_kernel_excess: Some(payment_excess),
            required_royalty_kernel_excess: None,
            signature: TransferAssetOp::sign("marketswap-punk", &buyer_pubkey, &Some(payment_excess), &None, &owner_secret),
        };
        let mut registry_after = std::collections::HashMap::new();
        registry_after.insert("marketswap-punk".to_string(), crate::core::assets::AssetRecord {
            asset_id: "marketswap-punk".to_string(),
            owner_pubkey: buyer_pubkey,
            metadata: vec![1u8; 4],
            minted_at_block: 1,
            collection_id: None,
        });

        let mut header2 = BlockHeader {
            height: 2,
            prev_hash: block1_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&registry_after),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        header2.validator_signature = Signature::sign(&header2.hash(), &private_key);
        let block2 = Block { header: header2, body: body2, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![transfer_op], launch_collection_ops: vec![], validator_ops: vec![] };

        assert!(chain_state.apply_block(&block2).is_applied(), "a payment and its conditional transfer must be acceptable in the same block");
        assert_eq!(chain_state.asset_registry["marketswap-punk"].owner_pubkey, buyer_pubkey);
    }

    #[test]
    fn rollback_makes_previously_satisfied_condition_unsatisfied_again() {
        use crate::core::assets::compute_asset_registry_root;

        let mut rng = OsRng;
        let (mut chain_state, block1_hash, _owner_secret, _owner_pubkey, _buyer_pubkey) = setup_chain_with_one_minted_asset(&mut rng);
        let private_key = Scalar::from(42u64);

        let (body2, payment_input, payment_excess) = build_body_with_payment(&mut rng, 2, 500);
        chain_state.utxos.insert(payment_input);
        let mut header2 = BlockHeader {
            height: 2,
            prev_hash: block1_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&{
                let mut r = std::collections::HashMap::new();
                r.insert("marketswap-punk".to_string(), chain_state.asset_registry["marketswap-punk"].clone());
                r
            }),
            collection_registry_root: crate::core::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        header2.validator_signature = Signature::sign(&header2.hash(), &private_key);
        let block2 = Block { header: header2, body: body2, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block2).is_applied());
        assert!(chain_state.kernel_excesses.contains(&payment_excess));

        // Roll back past the payment's block.
        assert!(chain_state.rollback_block().is_some());
        assert!(!chain_state.kernel_excesses.contains(&payment_excess), "rollback must remove the reverted block's kernel(s) from the index, or a condition could stay wrongly satisfied after a reorg");
        assert!(!chain_state.kernels.iter().any(|k| k.excess == payment_excess), "kernel_excesses must stay exactly in sync with kernels");
    }

    /// Full round trip for collection launches with scheduled multi-phase
    /// minting (see core::collections): launch a collection with a GTD
    /// (allowlisted) phase, an FCFS (allowlisted) phase, and a Public
    /// (open) phase; mint successfully in each; confirm a per-wallet-limit
    /// violation, an outside-the-window mint, and a bad Merkle proof are
    /// all rejected by apply_linear_block itself (not just soft-filtered
    /// earlier); then roll back and confirm collection_registry and
    /// collection_mint_counts are restored exactly.
    #[test]
    fn collection_launch_and_multi_phase_mint_round_trip() {
        use crate::core::assets::{MintAssetOp, AssetRecord, ASSET_MINT_FEE, compute_asset_registry_root};
        use crate::core::collections::{LaunchCollectionOp, MintPhase, compute_collection_registry_root, allowlist_leaf};
        use crate::core::merkle::{merkle_root, build_merkle_proof};
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let gens = PedersenGens::default();
        let private_key = Scalar::from(42u64);

        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        // --- Actors ---
        let creator_secret = Scalar::from(100u64);
        let creator_pubkey = Commitment(creator_secret * gens.B_blinding);

        let gtd_buyer_secret = Scalar::from(101u64);
        let gtd_buyer_pubkey = Commitment(gtd_buyer_secret * gens.B_blinding);
        let outsider_secret = Scalar::from(102u64);
        let outsider_pubkey = Commitment(outsider_secret * gens.B_blinding);
        let public_buyer_secret = Scalar::from(103u64);
        let public_buyer_pubkey = Commitment(public_buyer_secret * gens.B_blinding);

        // Allowlist for the GTD phase contains only gtd_buyer_pubkey.
        let gtd_allowlist_leaves = vec![allowlist_leaf(&gtd_buyer_pubkey)];
        let gtd_root = merkle_root(&gtd_allowlist_leaves);
        let gtd_proof = build_merkle_proof(&gtd_allowlist_leaves, 0);

        let phases = vec![
            MintPhase { name: "GTD".to_string(), start_time: 100, end_time: 200, price: 10, per_wallet_limit: 1, allowlist_merkle_root: Some(gtd_root) },
            MintPhase { name: "FCFS".to_string(), start_time: 200, end_time: 300, price: 5, per_wallet_limit: 2, allowlist_merkle_root: None },
            MintPhase { name: "Public".to_string(), start_time: 300, end_time: 400, price: 1, per_wallet_limit: 5, allowlist_merkle_root: None },
        ];
        let collection_id = "dropcol".to_string();
        let launch_signature = LaunchCollectionOp::sign(&collection_id, &creator_pubkey, "Drop Collection", "DROP", b"a scheduled drop", &phases, 0, &creator_secret);
        let launch_op = LaunchCollectionOp {
            collection_id: collection_id.clone(),
            creator_pubkey,
            name: "Drop Collection".to_string(),
            symbol: "DROP".to_string(),
            metadata: b"a scheduled drop".to_vec(),
            phases: phases.clone(),
            royalty_bps: 0,
            signature: launch_signature,
        };

        // Helper: builds a standalone-valid, creator-approved MintAssetOp
        // against `collection_id`/`phase_index`, conditioned on a real
        // payment kernel (see build_body_with_payment below) - exercises
        // the full trustless mechanism (payment gate + creator approval),
        // not just the phase/allowlist/quota gates.
        #[allow(clippy::too_many_arguments)]
        let make_mint_op = |rng: &mut OsRng, chain_state: &mut ChainState, asset_id: &str, owner_secret: &Scalar, owner_pubkey: Commitment, phase_index: u32, proof: Option<Vec<[u8; 32]>>, leaf_index: Option<u32>, required_excess: Commitment| -> MintAssetOp {
            let r_in = Scalar::random(rng);
            let input_commitment = Commitment::new(ASSET_MINT_FEE, r_in);
            chain_state.utxos.insert(input_commitment);
            let fee_payment = Transaction {
                inputs: vec![Input { commitment: input_commitment }],
                outputs: vec![],
                kernels: vec![TxKernel {
                    excess: Commitment::new(0, r_in),
                    fee: ASSET_MINT_FEE,
                    signature: Signature::sign(&ASSET_MINT_FEE.to_le_bytes(), &r_in),
                }],
            };
            let metadata = vec![9u8; 4];
            let collection_id_opt = Some(collection_id.clone());
            let phase_index_opt = Some(phase_index);
            let required_excess_opt = Some(required_excess);
            let signature = MintAssetOp::sign(asset_id, &metadata, &collection_id_opt, &phase_index_opt, &required_excess_opt, owner_secret);
            let creator_signature = MintAssetOp::sign_collection_approval(asset_id, &collection_id, phase_index, &required_excess, &owner_pubkey, &creator_secret);
            MintAssetOp {
                asset_id: asset_id.to_string(),
                owner_pubkey,
                metadata,
                fee_payment,
                collection_id: collection_id_opt,
                phase_index: phase_index_opt,
                allowlist_proof: proof,
                allowlist_leaf_index: leaf_index,
                required_kernel_excess: required_excess_opt,
                signature,
                creator_signature: Some(creator_signature),
            }
        };

        // Helper: assembles and applies a block at a given height/timestamp
        // from an already-built body (so its own kernel(s) - e.g. a payment
        // a mint_op's required_kernel_excess references - are whatever the
        // caller already arranged), returning whether apply_block accepted it.
        let apply_mint_block = |chain_state: &mut ChainState, height: u64, prev_hash: [u8; 32], timestamp: u64, body: Transaction, launch_ops: Vec<LaunchCollectionOp>, mint_op: Option<MintAssetOp>, expected_registry: &std::collections::HashMap<String, AssetRecord>, expected_collections: &std::collections::HashMap<String, crate::core::collections::CollectionRecord>| -> bool {
            let mut header = BlockHeader {
                height,
                prev_hash,
                total_kernel_offset: Scalar::zero(),
                nonce: 0,
                timestamp,
                validator_commitment: Commitment::new(1_000_000, private_key),
                validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
                name_registry_root: empty_registry_root(),
                chain_id: crate::core::genesis::CHAIN_ID,
                asset_registry_root: compute_asset_registry_root(expected_registry),
                collection_registry_root: compute_collection_registry_root(expected_collections),
            };
            header.validator_signature = Signature::sign(&header.hash(), &private_key);
            let block = Block {
                header, body,
                name_ops: vec![], transfer_ops: vec![],
                mint_ops: mint_op.into_iter().collect(),
                transfer_asset_ops: vec![],
                launch_collection_ops: launch_ops, validator_ops: vec![],
            };
            chain_state.apply_block(&block).is_applied()
        };

        // --- Block 1: launch the collection (timestamp before any phase - launching itself isn't time-gated). ---
        let mut collections_after_launch = std::collections::HashMap::new();
        collections_after_launch.insert(collection_id.clone(), crate::core::collections::CollectionRecord {
            collection_id: collection_id.clone(), creator_pubkey, name: "Drop Collection".to_string(), symbol: "DROP".to_string(),
            metadata: b"a scheduled drop".to_vec(), phases: phases.clone(), launched_at_block: 1, royalty_bps: 0,
        });
        let (body1, _) = build_coinbase_only_body(&mut rng, 1);
        assert!(apply_mint_block(&mut chain_state, 1, genesis_hash, 50, body1, vec![launch_op], None, &std::collections::HashMap::new(), &collections_after_launch), "launching the collection must apply");
        let block1_hash = chain_state.last_block_hash;
        assert!(chain_state.collection_registry.contains_key(&collection_id));

        // --- Block 2: successful GTD mint (timestamp 150, within [100,200), valid proof, real bundled payment). ---
        let (body2, payment_input2, payment_excess2) = build_body_with_payment(&mut rng, 2, 50);
        chain_state.utxos.insert(payment_input2);
        let gtd_mint = make_mint_op(&mut rng, &mut chain_state, "drop-gtd-1", &gtd_buyer_secret, gtd_buyer_pubkey, 0, Some(gtd_proof.clone()), Some(0), payment_excess2);
        let mut registry_after_gtd = std::collections::HashMap::new();
        registry_after_gtd.insert("drop-gtd-1".to_string(), AssetRecord { asset_id: "drop-gtd-1".to_string(), owner_pubkey: gtd_buyer_pubkey, metadata: gtd_mint.metadata.clone(), minted_at_block: 2, collection_id: Some(collection_id.clone()) });
        assert!(apply_mint_block(&mut chain_state, 2, block1_hash, 150, body2, vec![], Some(gtd_mint), &registry_after_gtd, &collections_after_launch), "a valid allowlisted, creator-approved GTD mint within its window must apply");
        let block2_hash = chain_state.last_block_hash;
        assert_eq!(chain_state.asset_registry["drop-gtd-1"].owner_pubkey, gtd_buyer_pubkey);
        let owner_bytes = |c: &Commitment| *c.as_point().compress().as_bytes();
        assert_eq!(chain_state.collection_mint_counts.get(&(collection_id.clone(), 0u32, owner_bytes(&gtd_buyer_pubkey))), Some(&1u32));

        // --- Rejection: a second GTD mint from the same wallet exceeds per_wallet_limit=1. ---
        let (body3a, payment_input3a, payment_excess3a) = build_body_with_payment(&mut rng, 3, 50);
        chain_state.utxos.insert(payment_input3a);
        let gtd_mint_again = make_mint_op(&mut rng, &mut chain_state, "drop-gtd-2", &gtd_buyer_secret, gtd_buyer_pubkey, 0, Some(gtd_proof.clone()), Some(0), payment_excess3a);
        let mut registry_over_limit = registry_after_gtd.clone();
        registry_over_limit.insert("drop-gtd-2".to_string(), AssetRecord { asset_id: "drop-gtd-2".to_string(), owner_pubkey: gtd_buyer_pubkey, metadata: gtd_mint_again.metadata.clone(), minted_at_block: 3, collection_id: Some(collection_id.clone()) });
        assert!(!apply_mint_block(&mut chain_state, 3, block2_hash, 150, body3a, vec![], Some(gtd_mint_again), &registry_over_limit, &collections_after_launch), "a second GTD mint from the same wallet must be rejected (per_wallet_limit=1)");

        // --- Rejection: minting the FCFS phase's index while still inside the GTD time window (timestamp 150 is outside FCFS's [200,300)). ---
        let (body3b, payment_input3b, payment_excess3b) = build_body_with_payment(&mut rng, 3, 50);
        chain_state.utxos.insert(payment_input3b);
        let fcfs_too_early = make_mint_op(&mut rng, &mut chain_state, "drop-fcfs-early", &outsider_secret, outsider_pubkey, 1, None, None, payment_excess3b);
        let mut registry_fcfs_early = registry_after_gtd.clone();
        registry_fcfs_early.insert("drop-fcfs-early".to_string(), AssetRecord { asset_id: "drop-fcfs-early".to_string(), owner_pubkey: outsider_pubkey, metadata: fcfs_too_early.metadata.clone(), minted_at_block: 3, collection_id: Some(collection_id.clone()) });
        assert!(!apply_mint_block(&mut chain_state, 3, block2_hash, 150, body3b, vec![], Some(fcfs_too_early), &registry_fcfs_early, &collections_after_launch), "a mint attempted before its phase's start_time must be rejected");

        // --- Rejection: a GTD mint from a non-allowlisted wallet (bad Merkle proof). ---
        let (body3c, payment_input3c, payment_excess3c) = build_body_with_payment(&mut rng, 3, 50);
        chain_state.utxos.insert(payment_input3c);
        let bad_proof_mint = make_mint_op(&mut rng, &mut chain_state, "drop-gtd-bad", &outsider_secret, outsider_pubkey, 0, Some(gtd_proof.clone()), Some(0), payment_excess3c);
        let mut registry_bad_proof = registry_after_gtd.clone();
        registry_bad_proof.insert("drop-gtd-bad".to_string(), AssetRecord { asset_id: "drop-gtd-bad".to_string(), owner_pubkey: outsider_pubkey, metadata: bad_proof_mint.metadata.clone(), minted_at_block: 3, collection_id: Some(collection_id.clone()) });
        assert!(!apply_mint_block(&mut chain_state, 3, block2_hash, 150, body3c, vec![], Some(bad_proof_mint), &registry_bad_proof, &collections_after_launch), "a mint from a non-allowlisted pubkey (proof doesn't verify against its root) must be rejected");

        // --- Rejection: a mint whose creator_signature doesn't match (forged/missing creator approval). ---
        let (body3d, payment_input3d, payment_excess3d) = build_body_with_payment(&mut rng, 3, 50);
        chain_state.utxos.insert(payment_input3d);
        let mut unapproved_mint = make_mint_op(&mut rng, &mut chain_state, "drop-unapproved", &outsider_secret, outsider_pubkey, 2, None, None, payment_excess3d);
        unapproved_mint.creator_signature = Some(MintAssetOp::sign("irrelevant", b"x", &None, &None, &None, &outsider_secret)); // garbage, not a real creator approval
        let mut registry_unapproved = registry_after_gtd.clone();
        registry_unapproved.insert("drop-unapproved".to_string(), AssetRecord { asset_id: "drop-unapproved".to_string(), owner_pubkey: outsider_pubkey, metadata: unapproved_mint.metadata.clone(), minted_at_block: 3, collection_id: Some(collection_id.clone()) });
        assert!(!apply_mint_block(&mut chain_state, 3, block2_hash, 350, body3d, vec![], Some(unapproved_mint), &registry_unapproved, &collections_after_launch), "a mint without a valid creator_signature must be rejected, even if everything else about it is valid");

        // --- Block 3: successful Public-phase mint (timestamp 350, within [300,400), no proof needed, real bundled payment). ---
        let (body3, payment_input3, payment_excess3) = build_body_with_payment(&mut rng, 3, 50);
        chain_state.utxos.insert(payment_input3);
        let public_mint = make_mint_op(&mut rng, &mut chain_state, "drop-public-1", &public_buyer_secret, public_buyer_pubkey, 2, None, None, payment_excess3);
        let mut registry_after_public = registry_after_gtd.clone();
        registry_after_public.insert("drop-public-1".to_string(), AssetRecord { asset_id: "drop-public-1".to_string(), owner_pubkey: public_buyer_pubkey, metadata: public_mint.metadata.clone(), minted_at_block: 3, collection_id: Some(collection_id.clone()) });
        assert!(apply_mint_block(&mut chain_state, 3, block2_hash, 350, body3, vec![], Some(public_mint), &registry_after_public, &collections_after_launch), "an open Public-phase mint within its window must apply");
        assert_eq!(chain_state.asset_registry["drop-public-1"].owner_pubkey, public_buyer_pubkey);
        assert_eq!(chain_state.collection_mint_counts.get(&(collection_id.clone(), 2u32, owner_bytes(&public_buyer_pubkey))), Some(&1u32));

        // --- Rollback: undo the Public-phase mint block, confirm exact state restoration. ---
        assert!(chain_state.rollback_block().is_some());
        assert!(!chain_state.asset_registry.contains_key("drop-public-1"), "rollback must un-mint the Public-phase asset");
        assert_eq!(chain_state.collection_mint_counts.get(&(collection_id.clone(), 2u32, owner_bytes(&public_buyer_pubkey))), None, "rollback must remove the mint count entirely (it was the wallet's only mint in this phase)");
        assert!(chain_state.collection_registry.contains_key(&collection_id), "the collection launch itself is from an earlier block and must survive this rollback");
        assert_eq!(chain_state.collection_mint_counts.get(&(collection_id.clone(), 0u32, owner_bytes(&gtd_buyer_pubkey))), Some(&1u32), "the earlier GTD mint's count must be untouched by rolling back a later block");

        // --- Rollback further: undo the GTD mint block, confirm its count clears too. ---
        assert!(chain_state.rollback_block().is_some());
        assert!(!chain_state.asset_registry.contains_key("drop-gtd-1"));
        assert_eq!(chain_state.collection_mint_counts.get(&(collection_id.clone(), 0u32, owner_bytes(&gtd_buyer_pubkey))), None);
        assert!(chain_state.collection_registry.contains_key(&collection_id), "the launch block hasn't been rolled back yet");

        // --- Rollback all the way past the launch block itself. ---
        assert!(chain_state.rollback_block().is_some());
        assert!(!chain_state.collection_registry.contains_key(&collection_id), "rolling back the launch block must un-launch the collection");
    }

    /// A resale of an asset minted from a royalty-charging collection must
    /// carry BOTH the seller's payment condition AND the creator's royalty
    /// condition - either one missing or unsatisfied blocks the whole
    /// transfer, mirroring required_kernel_excess's own trustless-payment
    /// guarantee but for a second, independent beneficiary. See
    /// core::assets::TransferAssetOp::required_royalty_kernel_excess.
    #[test]
    fn transfer_of_royalty_bearing_asset_requires_both_payment_kernels() {
        use crate::core::assets::{MintAssetOp, TransferAssetOp, AssetRecord, ASSET_MINT_FEE, compute_asset_registry_root};
        use crate::core::collections::{LaunchCollectionOp, CollectionRecord, MintPhase, compute_collection_registry_root};
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        let gens = PedersenGens::default();
        let private_key = Scalar::from(42u64);

        let creator_secret = Scalar::from(55u64);
        let creator_pubkey = Commitment(creator_secret * gens.B_blinding);
        let owner_secret = Scalar::random(&mut rng);
        let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
        let buyer_secret = Scalar::random(&mut rng);
        let buyer_pubkey = Commitment(buyer_secret * gens.B_blinding);

        let phases = vec![MintPhase { name: "Public".to_string(), start_time: 0, end_time: 10_000, price: 1, per_wallet_limit: 10, allowlist_merkle_root: None }];
        let collection_id = "royaltycol".to_string();
        let royalty_bps = 1000u16; // 10%
        let launch_signature = LaunchCollectionOp::sign(&collection_id, &creator_pubkey, "Royalty Collection", "ROY", b"", &phases, royalty_bps, &creator_secret);
        let launch_op = LaunchCollectionOp {
            collection_id: collection_id.clone(), creator_pubkey, name: "Royalty Collection".to_string(), symbol: "ROY".to_string(),
            metadata: vec![], phases: phases.clone(), royalty_bps, signature: launch_signature,
        };

        let mut collections_after_launch = std::collections::HashMap::new();
        collections_after_launch.insert(collection_id.clone(), CollectionRecord {
            collection_id: collection_id.clone(), creator_pubkey, name: "Royalty Collection".to_string(), symbol: "ROY".to_string(),
            metadata: vec![], phases: phases.clone(), launched_at_block: 1, royalty_bps,
        });
        let (body1, _) = build_coinbase_only_body(&mut rng, 1);
        let mut header1 = BlockHeader {
            height: 1, prev_hash: genesis_hash, total_kernel_offset: Scalar::zero(), nonce: 0, timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key), validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(), chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: compute_collection_registry_root(&collections_after_launch),
        };
        header1.validator_signature = Signature::sign(&header1.hash(), &private_key);
        let block1 = Block { header: header1, body: body1, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![launch_op], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block1).is_applied(), "launching the royalty-charging collection must apply");
        let block1_hash = chain_state.last_block_hash;

        // --- Block 2: mint the asset. A collection mint always requires a
        // real payment kernel + creator approval regardless of phase.price
        // (see the mint gate's own doc comment) - reuse build_body_with_payment
        // for that condition; consensus never checks who a kernel actually
        // paid, only that it exists, so this stands in for "the mint price
        // landed somewhere real" without needing to model the full payment
        // flow here too.
        let (body2, mint_payment_input, mint_payment_excess) = build_body_with_payment(&mut rng, 2, 1);
        chain_state.utxos.insert(mint_payment_input);
        let r_fee = Scalar::random(&mut rng);
        let fee_input = Commitment::new(ASSET_MINT_FEE, r_fee);
        chain_state.utxos.insert(fee_input);
        let fee_payment = Transaction {
            inputs: vec![Input { commitment: fee_input }],
            outputs: vec![],
            kernels: vec![TxKernel { excess: Commitment::new(0, r_fee), fee: ASSET_MINT_FEE, signature: Signature::sign(&ASSET_MINT_FEE.to_le_bytes(), &r_fee) }],
        };
        let metadata = vec![9u8; 4];
        let collection_id_opt = Some(collection_id.clone());
        let phase_index_opt = Some(0u32);
        let required_excess_opt = Some(mint_payment_excess);
        let mint_signature = MintAssetOp::sign("royalty-punk", &metadata, &collection_id_opt, &phase_index_opt, &required_excess_opt, &owner_secret);
        let creator_signature = MintAssetOp::sign_collection_approval("royalty-punk", &collection_id, 0, &mint_payment_excess, &owner_pubkey, &creator_secret);
        let mint_op = MintAssetOp {
            asset_id: "royalty-punk".to_string(), owner_pubkey, metadata, fee_payment,
            collection_id: collection_id_opt, phase_index: phase_index_opt,
            allowlist_proof: None, allowlist_leaf_index: None,
            required_kernel_excess: required_excess_opt, signature: mint_signature, creator_signature: Some(creator_signature),
        };
        let mut registry_after_mint = std::collections::HashMap::new();
        registry_after_mint.insert("royalty-punk".to_string(), AssetRecord {
            asset_id: "royalty-punk".to_string(), owner_pubkey, metadata: mint_op.metadata.clone(), minted_at_block: 2,
            collection_id: Some(collection_id.clone()),
        });
        let mut header2 = BlockHeader {
            height: 2, prev_hash: block1_hash, total_kernel_offset: Scalar::zero(), nonce: 0, timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key), validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(), chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&registry_after_mint),
            collection_registry_root: compute_collection_registry_root(&collections_after_launch),
        };
        header2.validator_signature = Signature::sign(&header2.hash(), &private_key);
        let block2 = Block { header: header2, body: body2, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![mint_op], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block2).is_applied(), "the royalty-collection mint must apply");
        let block2_hash = chain_state.last_block_hash;
        assert_eq!(chain_state.asset_registry["royalty-punk"].collection_id, Some(collection_id.clone()));

        // --- Block 3 (rejected): a resale whose seller payment lands but
        // omits the royalty condition entirely must be rejected outright. ---
        let (body3_missing, seller_input_missing, seller_excess_missing) = build_body_with_payment(&mut rng, 3, 90);
        chain_state.utxos.insert(seller_input_missing);
        let transfer_missing_royalty = TransferAssetOp {
            asset_id: "royalty-punk".to_string(),
            new_owner_pubkey: buyer_pubkey,
            required_kernel_excess: Some(seller_excess_missing),
            required_royalty_kernel_excess: None,
            signature: TransferAssetOp::sign("royalty-punk", &buyer_pubkey, &Some(seller_excess_missing), &None, &owner_secret),
        };
        let mut registry_after_transfer = std::collections::HashMap::new();
        registry_after_transfer.insert("royalty-punk".to_string(), AssetRecord {
            asset_id: "royalty-punk".to_string(), owner_pubkey: buyer_pubkey, metadata: vec![9u8; 4], minted_at_block: 2,
            collection_id: Some(collection_id.clone()),
        });
        let mut header3_missing = BlockHeader {
            height: 3, prev_hash: block2_hash, total_kernel_offset: Scalar::zero(), nonce: 0, timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key), validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(), chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&registry_after_transfer),
            collection_registry_root: compute_collection_registry_root(&collections_after_launch),
        };
        header3_missing.validator_signature = Signature::sign(&header3_missing.hash(), &private_key);
        let block3_missing = Block { header: header3_missing, body: body3_missing, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![transfer_missing_royalty], launch_collection_ops: vec![], validator_ops: vec![] };
        assert!(!chain_state.apply_block(&block3_missing).is_applied(), "a resale of a royalty-bearing asset without a royalty condition must be rejected, even though the seller's own payment landed");
        assert_eq!(chain_state.asset_registry["royalty-punk"].owner_pubkey, owner_pubkey, "ownership must not have changed");

        // --- Block 3 (accepted): both the seller's payment AND the
        // creator's royalty payment land in the same block as the transfer -
        // the one-block atomic swap case, extended to two beneficiaries. ---
        let (body3, seller_input, seller_excess) = build_body_with_payment(&mut rng, 3, 90);
        chain_state.utxos.insert(seller_input);
        let r_royalty_in = Scalar::random(&mut rng);
        let r_royalty_out = Scalar::random(&mut rng);
        let royalty_input_commitment = Commitment::new(10, r_royalty_in);
        let royalty_output = Output { commitment: Commitment::new(10, r_royalty_out), proof: RangeProof::prove(10, &r_royalty_out), note: vec![] };
        let royalty_excess_r = r_royalty_in - r_royalty_out;
        let royalty_kernel = TxKernel { excess: Commitment::new(0, royalty_excess_r), fee: 0, signature: Signature::sign(&0u64.to_le_bytes(), &royalty_excess_r) };
        let royalty_excess = royalty_kernel.excess;
        chain_state.utxos.insert(royalty_input_commitment);
        let mut body3_with_royalty = body3;
        body3_with_royalty.inputs.push(Input { commitment: royalty_input_commitment });
        body3_with_royalty.outputs.push(royalty_output);
        body3_with_royalty.kernels.push(royalty_kernel);

        let transfer_op = TransferAssetOp {
            asset_id: "royalty-punk".to_string(),
            new_owner_pubkey: buyer_pubkey,
            required_kernel_excess: Some(seller_excess),
            required_royalty_kernel_excess: Some(royalty_excess),
            signature: TransferAssetOp::sign("royalty-punk", &buyer_pubkey, &Some(seller_excess), &Some(royalty_excess), &owner_secret),
        };
        let mut header3 = BlockHeader {
            height: 3, prev_hash: block2_hash, total_kernel_offset: Scalar::zero(), nonce: 0, timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key), validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(), chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&registry_after_transfer),
            collection_registry_root: compute_collection_registry_root(&collections_after_launch),
        };
        header3.validator_signature = Signature::sign(&header3.hash(), &private_key);
        let block3 = Block { header: header3, body: body3_with_royalty, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![transfer_op], launch_collection_ops: vec![], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block3).is_applied(), "a resale with BOTH the seller's payment and the creator's royalty payment present must apply");
        assert_eq!(chain_state.asset_registry["royalty-punk"].owner_pubkey, buyer_pubkey, "ownership must transfer once both conditions are satisfied");
    }

    /// A royalty only applies to an actual SALE (required_kernel_excess is
    /// Some) - an unconditional transfer of a royalty-bearing asset (a
    /// gift, an airdrop, moving it between your own wallets) must still go
    /// through for free, with no royalty payment required at all. Without
    /// this exemption, a royalty-charging collection's assets could never
    /// be gifted or moved without also paying the creator, which isn't
    /// what "royalty on resale" is supposed to mean.
    #[test]
    fn unconditional_transfer_of_royalty_bearing_asset_needs_no_royalty_payment() {
        use crate::core::assets::{MintAssetOp, TransferAssetOp, AssetRecord, ASSET_MINT_FEE, compute_asset_registry_root};
        use crate::core::collections::{LaunchCollectionOp, CollectionRecord, MintPhase, compute_collection_registry_root};
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let mut chain_state = ChainState::new();
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block).is_applied());

        let gens = PedersenGens::default();
        let private_key = Scalar::from(42u64);

        let creator_secret = Scalar::from(66u64);
        let creator_pubkey = Commitment(creator_secret * gens.B_blinding);
        let owner_secret = Scalar::random(&mut rng);
        let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
        let friend_secret = Scalar::random(&mut rng);
        let friend_pubkey = Commitment(friend_secret * gens.B_blinding);

        let phases = vec![MintPhase { name: "Public".to_string(), start_time: 0, end_time: 10_000, price: 1, per_wallet_limit: 10, allowlist_merkle_root: None }];
        let collection_id = "giftablecol".to_string();
        let royalty_bps = 1000u16; // 10%
        let launch_signature = LaunchCollectionOp::sign(&collection_id, &creator_pubkey, "Giftable Collection", "GIFT", b"", &phases, royalty_bps, &creator_secret);
        let launch_op = LaunchCollectionOp {
            collection_id: collection_id.clone(), creator_pubkey, name: "Giftable Collection".to_string(), symbol: "GIFT".to_string(),
            metadata: vec![], phases: phases.clone(), royalty_bps, signature: launch_signature,
        };
        let mut collections_after_launch = std::collections::HashMap::new();
        collections_after_launch.insert(collection_id.clone(), CollectionRecord {
            collection_id: collection_id.clone(), creator_pubkey, name: "Giftable Collection".to_string(), symbol: "GIFT".to_string(),
            metadata: vec![], phases: phases.clone(), launched_at_block: 1, royalty_bps,
        });
        let (body1, _) = build_coinbase_only_body(&mut rng, 1);
        let mut header1 = BlockHeader {
            height: 1, prev_hash: genesis_hash, total_kernel_offset: Scalar::zero(), nonce: 0, timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key), validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(), chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: compute_collection_registry_root(&collections_after_launch),
        };
        header1.validator_signature = Signature::sign(&header1.hash(), &private_key);
        let block1 = Block { header: header1, body: body1, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![], launch_collection_ops: vec![launch_op], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block1).is_applied());
        let block1_hash = chain_state.last_block_hash;

        let (body2, mint_payment_input, mint_payment_excess) = build_body_with_payment(&mut rng, 2, 1);
        chain_state.utxos.insert(mint_payment_input);
        let r_fee = Scalar::random(&mut rng);
        let fee_input = Commitment::new(ASSET_MINT_FEE, r_fee);
        chain_state.utxos.insert(fee_input);
        let fee_payment = Transaction {
            inputs: vec![Input { commitment: fee_input }],
            outputs: vec![],
            kernels: vec![TxKernel { excess: Commitment::new(0, r_fee), fee: ASSET_MINT_FEE, signature: Signature::sign(&ASSET_MINT_FEE.to_le_bytes(), &r_fee) }],
        };
        let metadata = vec![3u8; 4];
        let collection_id_opt = Some(collection_id.clone());
        let phase_index_opt = Some(0u32);
        let required_excess_opt = Some(mint_payment_excess);
        let mint_signature = MintAssetOp::sign("gift-punk", &metadata, &collection_id_opt, &phase_index_opt, &required_excess_opt, &owner_secret);
        let creator_signature = MintAssetOp::sign_collection_approval("gift-punk", &collection_id, 0, &mint_payment_excess, &owner_pubkey, &creator_secret);
        let mint_op = MintAssetOp {
            asset_id: "gift-punk".to_string(), owner_pubkey, metadata, fee_payment,
            collection_id: collection_id_opt, phase_index: phase_index_opt,
            allowlist_proof: None, allowlist_leaf_index: None,
            required_kernel_excess: required_excess_opt, signature: mint_signature, creator_signature: Some(creator_signature),
        };
        let mut registry_after_mint = std::collections::HashMap::new();
        registry_after_mint.insert("gift-punk".to_string(), AssetRecord {
            asset_id: "gift-punk".to_string(), owner_pubkey, metadata: mint_op.metadata.clone(), minted_at_block: 2,
            collection_id: Some(collection_id.clone()),
        });
        let mut header2 = BlockHeader {
            height: 2, prev_hash: block1_hash, total_kernel_offset: Scalar::zero(), nonce: 0, timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key), validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(), chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&registry_after_mint),
            collection_registry_root: compute_collection_registry_root(&collections_after_launch),
        };
        header2.validator_signature = Signature::sign(&header2.hash(), &private_key);
        let block2 = Block { header: header2, body: body2, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![mint_op], transfer_asset_ops: vec![], launch_collection_ops: vec![], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block2).is_applied());
        let block2_hash = chain_state.last_block_hash;

        // --- Block 3: an unconditional gift transfer - no payment, no
        // royalty, and it must still apply. ---
        let (body3, _) = build_coinbase_only_body(&mut rng, 3);
        let gift_transfer = TransferAssetOp {
            asset_id: "gift-punk".to_string(),
            new_owner_pubkey: friend_pubkey,
            required_kernel_excess: None,
            required_royalty_kernel_excess: None,
            signature: TransferAssetOp::sign("gift-punk", &friend_pubkey, &None, &None, &owner_secret),
        };
        let mut registry_after_gift = std::collections::HashMap::new();
        registry_after_gift.insert("gift-punk".to_string(), AssetRecord {
            asset_id: "gift-punk".to_string(), owner_pubkey: friend_pubkey, metadata: vec![3u8; 4], minted_at_block: 2,
            collection_id: Some(collection_id.clone()),
        });
        let mut header3 = BlockHeader {
            height: 3, prev_hash: block2_hash, total_kernel_offset: Scalar::zero(), nonce: 0, timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key), validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(), chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: compute_asset_registry_root(&registry_after_gift),
            collection_registry_root: compute_collection_registry_root(&collections_after_launch),
        };
        header3.validator_signature = Signature::sign(&header3.hash(), &private_key);
        let block3 = Block { header: header3, body: body3, name_ops: vec![], transfer_ops: vec![], mint_ops: vec![], transfer_asset_ops: vec![gift_transfer], launch_collection_ops: vec![], validator_ops: vec![] };
        assert!(chain_state.apply_block(&block3).is_applied(), "an unconditional gift transfer of a royalty-bearing asset must apply with no royalty payment at all");
        assert_eq!(chain_state.asset_registry["gift-punk"].owner_pubkey, friend_pubkey);
    }
}

