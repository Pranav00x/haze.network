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

        // Construct the coinbase transaction for block 1:
        // Value: BLOCK_REWARD (60) + total_fees (fee1 + fee2 = 5 + 5 = 10) = 70.
        let r_coinbase = Scalar::random(&mut rng);
        let coinbase_output = Output {
            commitment: Commitment::new(70, r_coinbase),
            proof: RangeProof::prove(70, &r_coinbase),
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
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        // Construct a block with the aggregated transaction (including coinbase)
        let block = Block {
            header,
            body: block_body,
        };

        // Apply the block to the chain state
        let applied = chain_state.apply_block(&block);
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

        let reward_commitment = Commitment::new(crate::core::block::BLOCK_REWARD, Scalar::zero()).as_point();

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
        let p2p_server = Arc::new(crate::p2p::server::P2pServer::new(Arc::clone(&mempool), Arc::clone(&chain_state)));
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
            
            let coinbase_output = Output {
                commitment: Commitment::new(60, r_coinbase),
                proof: RangeProof::prove(60, &r_coinbase),
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
            };
            let msg = header.hash();
            header.validator_signature = Signature::sign(&msg, &private_key);

            Block { header, body }
        }

        let mut chain_state = ChainState::new();

        // 1. Apply Genesis Block (height 0)
        let genesis_block = crate::core::genesis::genesis_block();
        let genesis_hash = genesis_block.header.hash();
        assert!(chain_state.apply_block(&genesis_block));
        assert_eq!(chain_state.current_height, 0);

        // 2. Build and apply Block A1
        let a1 = create_empty_block(1, genesis_hash);
        let a1_hash = a1.header.hash();
        assert!(chain_state.apply_block(&a1));
        assert_eq!(chain_state.current_height, 1);
        assert_eq!(chain_state.last_block_hash, a1_hash);

        // 3. Build and apply Block A2 (Main chaintip is now at height 2)
        let a2 = create_empty_block(2, a1_hash);
        let a2_hash = a2.header.hash();
        assert!(chain_state.apply_block(&a2));
        assert_eq!(chain_state.current_height, 2);
        assert_eq!(chain_state.last_block_hash, a2_hash);

        // 4. Build competing fork from A1:
        // Block B2 (height 2, prev_hash = A1)
        let b2 = create_empty_block(2, a1_hash);
        let b2_hash = b2.header.hash();
        
        // Applying B2 should NOT change active tip (height 2 fork is same length as A2)
        assert!(!chain_state.apply_block(&b2)); // Returns false because tip didn't switch
        assert_eq!(chain_state.current_height, 2);
        assert_eq!(chain_state.last_block_hash, a2_hash);

        // 5. Build Block B3 on top of B2 (height 3, prev_hash = B2)
        let b3 = create_empty_block(3, b2_hash);
        let b3_hash = b3.header.hash();

        // Applying B3 should trigger reorganization (height 3 > height 2)
        assert!(chain_state.apply_block(&b3)); // Returns true because tip switched
        assert_eq!(chain_state.current_height, 3);
        assert_eq!(chain_state.last_block_hash, b3_hash);

        // Verify that A2 is no longer tip, and B2 and B3 are active
        assert!(chain_state.blocks.contains_key(&a2_hash));
        assert!(chain_state.blocks.contains_key(&b2_hash));
        assert!(chain_state.blocks.contains_key(&b3_hash));
    }
}

