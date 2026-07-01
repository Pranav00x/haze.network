use super::transaction::{Transaction, Input, Output, TxKernel};

/// Aggregates multiple transactions into one and performs cut-through
/// to remove matching inputs and outputs (internal spending).
pub fn aggregate_and_cut_through(txs: Vec<Transaction>) -> Transaction {
    let mut all_inputs: Vec<Input> = Vec::new();
    let mut all_outputs: Vec<Output> = Vec::new();
    let mut all_kernels: Vec<TxKernel> = Vec::new();
    
    for mut tx in txs {
        all_inputs.append(&mut tx.inputs);
        all_outputs.append(&mut tx.outputs);
        all_kernels.append(&mut tx.kernels);
    }
    
    // Perform cut-through
    let mut final_inputs = Vec::new();
    let mut output_consumed = vec![false; all_outputs.len()];
    
    for input in all_inputs {
        let mut matched = false;
        for (i, output) in all_outputs.iter().enumerate() {
            if !output_consumed[i] && output.commitment == input.commitment {
                // This input consumes an output created within the same block/pool
                output_consumed[i] = true;
                matched = true;
                break;
            }
        }
        // If it didn't match any output in the pool, it must refer to a past UTXO
        if !matched {
            final_inputs.push(input);
        }
    }
    
    let mut final_outputs = Vec::new();
    for (i, output) in all_outputs.into_iter().enumerate() {
        if !output_consumed[i] {
            final_outputs.push(output);
        }
    }
    
    Transaction {
        inputs: final_inputs,
        outputs: final_outputs,
        kernels: all_kernels,
    }
}
