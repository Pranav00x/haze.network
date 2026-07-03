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
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        // Construct a block with the aggregated transaction (including coinbase)
        let block = Block {
            header,
            body: block_body,
            name_ops: vec![],
            transfer_ops: vec![],
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
        assert!(chain_state.register_validator(commitment_a, 1000, r_a));
        assert!(chain_state.register_validator(commitment_b, 2000, r_b));
        assert!(chain_state.register_validator(commitment_c, 3000, r_c));

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
        let p2p_server = Arc::new(crate::p2p::server::P2pServer::new(Arc::clone(&mempool), Arc::clone(&chain_state), storage));
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

        // 5. Send RegisterValidator
        let reg_msg = crate::p2p::message::P2pMessage::RegisterValidator {
            commitment,
            value,
            blinding: r,
        };
        let bytes = bincode::serialize(&reg_msg).unwrap();
        let len = bytes.len() as u32;
        client_stream.write_all(&len.to_le_bytes()).await.unwrap();
        client_stream.write_all(&bytes).await.unwrap();
        client_stream.flush().await.unwrap();

        // Sleep to let server handle message
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 6. Verify that the validator was registered in the shared chain state!
        let c = chain_state.lock().unwrap();
        assert_eq!(c.active_validators.len(), 1);
        assert_eq!(c.active_validators[0].commitment, commitment);
        assert_eq!(c.active_validators[0].value, value);
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
            };
            let msg = header.hash();
            header.validator_signature = Signature::sign(&msg, &private_key);

            Block { header, body, name_ops: vec![], transfer_ops: vec![] }
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
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        let block = Block { header, body, name_ops: vec![op], transfer_ops: vec![] };

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
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        let block = Block { header, body, name_ops: vec![op1, op2], transfer_ops: vec![] };

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
        };
        header1.validator_signature = Signature::sign(&header1.hash(), &private_key);
        let block1 = Block { header: header1, body: body1, name_ops: vec![register_op], transfer_ops: vec![] };
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
        };
        header2.validator_signature = Signature::sign(&header2.hash(), &private_key);
        let block2 = Block { header: header2, body: body2, name_ops: vec![], transfer_ops: vec![transfer_op] };

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
        };
        header1.validator_signature = Signature::sign(&header1.hash(), &private_key);
        let block1 = Block { header: header1, body: body1, name_ops: vec![register_op], transfer_ops: vec![] };
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
        };
        header2.validator_signature = Signature::sign(&header2.hash(), &private_key);
        let block2 = Block { header: header2, body: body2, name_ops: vec![], transfer_ops: vec![transfer_op] };

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
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);
        let block = Block { header, body, name_ops: vec![], transfer_ops: vec![] };

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
        assert_eq!(genesis.body.outputs.len(), 5, "expected exactly 5 genesis outputs: validator stake + team + investor + airdrop + treasury");
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
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);
        let block = Block { header, body, name_ops: vec![], transfer_ops: vec![] };

        assert!(!chain_state.apply_block(&block).is_applied(), "a block with a mismatched chain_id must be rejected");
        assert_eq!(chain_state.current_height, 0, "chain must not have advanced past genesis");
    }
}

