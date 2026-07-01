use crate::crypto::pedersen::Commitment;
use crate::crypto::range_proof::RangeProof;
use crate::crypto::schnorr::Signature;
use curve25519_dalek::scalar::Scalar;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub commitment: Commitment,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub commitment: Commitment,
    pub proof: RangeProof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxKernel {
    pub excess: Commitment,
    pub fee: u64,
    pub signature: Signature,
}

#[derive(Clone, Debug)]
pub struct Transaction {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub kernels: Vec<TxKernel>,
}

impl Transaction {
    /// Validates the transaction:
    /// 1. Sum(Inputs) - Sum(Outputs) - Fee*H == Sum(Kernels)
    /// 2. All range proofs are valid
    /// 3. All kernel signatures are valid
    pub fn validate(&self) -> bool {
        // 1. Verify Range Proofs
        for output in &self.outputs {
            if !output.proof.verify(&output.commitment) {
                return false;
            }
        }
        
        // 2. Verify Kernel Signatures
        for kernel in &self.kernels {
            // For simplicity, the message signed is just the fee bytes
            let mut message = Vec::new();
            message.extend_from_slice(&kernel.fee.to_le_bytes());
            if !kernel.signature.verify(&message, &kernel.excess) {
                return false;
            }
        }
        
        // 3. Verify Equation: sum(inputs) - sum(outputs) - fee*H = sum(kernel_excess)
        let mut sum_inputs = curve25519_dalek::ristretto::RistrettoPoint::default();
        for input in &self.inputs {
            sum_inputs += input.commitment.as_point();
        }
        
        let mut sum_outputs = curve25519_dalek::ristretto::RistrettoPoint::default();
        for output in &self.outputs {
            sum_outputs += output.commitment.as_point();
        }
        
        let mut sum_kernels = curve25519_dalek::ristretto::RistrettoPoint::default();
        let mut total_fee = 0u64;
        for kernel in &self.kernels {
            sum_kernels += kernel.excess.as_point();
            total_fee += kernel.fee;
        }
        
        let fee_commitment = Commitment::new(total_fee, Scalar::ZERO).as_point();
        
        // We expect: sum_inputs - sum_outputs - fee_commitment = sum_kernels
        let expected_kernels = sum_inputs - sum_outputs - fee_commitment;
        
        expected_kernels == sum_kernels
    }
}
