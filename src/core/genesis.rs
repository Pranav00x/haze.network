use crate::core::block::{Block, BlockHeader};
use crate::core::transaction::{Transaction, Output, TxKernel};
use crate::crypto::pedersen::Commitment;
use crate::crypto::range_proof::RangeProof;
use crate::crypto::schnorr::Signature;
use bulletproofs::RangeProof as BpRangeProof;
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
// Blinding secrecy: every locked output below (team/investor tranches,
// airdrop, treasury) is backed by a real, randomly-generated secret - not a
// small hardcoded integer like the untouched validator-stake output
// (blinding=42, which is INTENTIONALLY public, a devnet claim-genesis
// convenience). Only the resulting PUBLIC data (commitment + range proof +
// kernel excess/signature) is committed here; the secrets themselves were
// generated once via src/bin/genesis_keygen.rs and handed off out-of-band -
// they do not exist anywhere in this repo or its history. This is possible
// because a Bulletproofs range proof only needs the secret to be
// *generated*, never to be *verified* - once the proof is embedded here,
// every node can validate it against the public commitment without ever
// knowing the blinding factor.
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
/// src/api/faucet.rs) - the faucet reconstructs its spendable balance at
/// runtime from a secret supplied via the HAZE_TREASURY_BLINDING env var
/// (see wallet::planner::blinding_for), never from a constant in this file.
pub const TREASURY_ALLOCATION: u64 = 630_000_000;

/// Total minted at genesis outside the block-reward schedule: the four
/// allocations above, plus the pre-existing 1,000,000 validator-stake /
/// claim-genesis convenience output (untouched by this reallocation - its
/// value and blinding=42 are hardcoded directly into consensus in several
/// places, see core::chain::select_proposer/apply_linear_block, so it isn't
/// part of the tokenomics split above; it's bootstrap plumbing, not supply).
pub const GENESIS_TOTAL_MINTED: u64 = 1_000_000
    + TEAM_ALLOCATION + INVESTOR_ALLOCATION + AIRDROP_ALLOCATION + TREASURY_ALLOCATION;

/// Team and investor allocations are each split into VESTING_TRANCHE_COUNT
/// equal tranches, one per vesting unlock height (see core::vesting) -
/// TEAM_ALLOCATION / INVESTOR_ALLOCATION both divide evenly by this count.
pub const VESTING_TRANCHE_COUNT: usize = 7;
pub const TEAM_TRANCHE_VALUE: u64 = TEAM_ALLOCATION / VESTING_TRANCHE_COUNT as u64;
pub const INVESTOR_TRANCHE_VALUE: u64 = INVESTOR_ALLOCATION / VESTING_TRANCHE_COUNT as u64;

// ---------------------------------------------------------------------------
// Network identity - enforced in BlockHeader::hash() (see core::block) so
// nodes on different networks can never accidentally interoperate, even if
// they somehow connected over P2P.
pub const CHAIN_ID: u64 = 1;
pub const NETWORK_NAME: &str = "haze-testnet-1";

/// The public data needed to reconstruct one locked genesis output without
/// ever knowing the secret blinding factor that produced it - see the
/// module doc comment and src/bin/genesis_keygen.rs.
pub struct LockedOutputData {
    pub value: u64,
    commitment_hex: &'static str,
    proof_hex: &'static str,
    excess_hex: &'static str,
    sig_s_hex: &'static str,
    sig_e_hex: &'static str,
}

impl LockedOutputData {
    /// The output's public commitment - cheap to compute, used by
    /// core::vesting to identify locked tranches without needing the full
    /// (Output, TxKernel) reconstruction.
    pub fn commitment(&self) -> Commitment {
        Commitment::from_hex(self.commitment_hex).expect("hardcoded genesis commitment must be valid hex")
    }
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hardcoded genesis hex must be valid"))
        .collect()
}

fn scalar_from_hex(s: &str) -> Scalar {
    let bytes = hex_decode(s);
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Scalar::from_bits(arr)
}

/// Reconstructs a locked output's (Output, TxKernel) pair entirely from its
/// public data - no secret blinding factor is ever read or needed here.
fn build_locked_output(data: &LockedOutputData) -> (Output, TxKernel) {
    let commitment = data.commitment();
    let proof_bytes = hex_decode(data.proof_hex);
    let proof = RangeProof(BpRangeProof::from_bytes(&proof_bytes).expect("hardcoded genesis range proof must be valid"));
    let output = Output { commitment, proof, note: vec![] };

    let excess = Commitment::from_hex(data.excess_hex).expect("hardcoded genesis kernel excess must be valid hex");
    let signature = Signature {
        s: scalar_from_hex(data.sig_s_hex),
        e: scalar_from_hex(data.sig_e_hex),
    };
    let kernel = TxKernel { excess, fee: 0, signature };

    (output, kernel)
}

/// Generated once via `cargo run --bin genesis_keygen`; the corresponding
/// secrets were handed off out-of-band and do not exist in this repo.
pub const TEAM_TRANCHES: [LockedOutputData; VESTING_TRANCHE_COUNT] = [
    LockedOutputData {
        value: TEAM_TRANCHE_VALUE,
        commitment_hex: "1aa6b8820e6fdf5a505d20506aac2a3a4699c3346e5c8ab52c3af26b541b7b5e",
        proof_hex: "d479dfb952adec9b447ff08917141ace8619f15cd085ff28b1af7e74908bdf262683d185095701e5c9019dc0406a6b532a1ef66d9df8e7de4b73d8a2927b2d73300635638c93a0889570e9c9a1456bc0d42a68a92b39c44363936216779909019a18617f252004e5e227afc166ea107cf779b9c00203d301295944757c88a153a254e2f555e34ed0d9a0f07b6069377800fec8a72883590115817cdcfb3f7a09e6b8c641e1d870ff552c064c676b36f5d848b979ca45ff7b12ed3185287453090539e1752bf07c3653b0751790249d00e28026c520a0f01d7aad8ee26a4d860ea43ec2a7f0a47055c219beeefa5821c28c004de0139faa9fdba91ef196a009259217d7f275c482ee9a0fa8f80c59c3762e72522a390f4bb0a1b7ace15077517b76fcfb84add5dc2f861a8533aa503f46c1e5eca44cd03173a6b5e92269f65f78be855b6422ada3b1be5e93af9bbb329ddda178084922040deb2ed7dcc3e04d6a2674ce6f90848cb1fb65baebe3fbc6cf1589462e84f418a34a5bcead90c5937242949f88f36b352be852e6b4396ef7288838e6208ecf1f7a20faa43f1c14c51f3e92436ed993de1c152fbc637b0cb480151ddf36e87c52f86b2a68097a5ce11888d2520aba2348ddb2f0cd0b21de844dd7c4b45878588ef5e84e53f8bf1d080d8a973adf98e43485f4ab9a40c8d1aaa0c5e2fad0695b6b04f92de2bef096696b7cd5fce6a2f296a51319ad76a8787b5fee117c982764e8de2188c112294b2d1a9026571376f2dd70c34a85580574c92074f016a32459fdc0852ffb54fc768d0d62116a7fcb4af7bdb90366e52c32aa388ba740f49f38b9a1716510e5975fb26736b83338b986f30f50c01e293ada2197dfda9213e7da3983f2ba7ef555e8b101ba8728678a2fe04f5f03174854f49902b49fd34f8a47b5822cc2512c677acc0e",
        excess_hex: "ac3d02cfee41271cdbc2dfbcf38a4ef5941c7e5555a7fd59660e507f0d49600e",
        sig_s_hex: "2d572f0c0b555f58ca1eef6dfc9b7a24b5779f6c98bab68fe1ca6d2d46df0503",
        sig_e_hex: "f4969733154f3a9acaff656512e4d44f169e1a8c739516084dae621902a5e206",
    },
    LockedOutputData {
        value: TEAM_TRANCHE_VALUE,
        commitment_hex: "4a38b99646f39d6a22753a3a50294f6a78039f3d66f402d106ecfc39e1310714",
        proof_hex: "744f8a6bd4baf55f267c7f1e68eff44624869ad3a1f288c11bf1157b03fbc72f1691228b89ec67e8d230ed7b9b3be2fccb444f719f3f3ab06cba0508a7bb7d59767ed47405ee068e0c6c93b559bb87354eaf7ba4fdea4038fbd82b8b0e0f3b461602e129abdbe8363a1845fb11c24bd1f2004d8b4d7d9fac8278c5603e935b269963587a512436b74cf69404de289c3f528ddfdbd897acdba9cacfdb75439b0f78c0c758d31091041e8b6dd2448b4be50ac08b59a60e011cae417bc063fa310cb4eb58c76c5a636e23c3affb96d49970c506c04694ccf3d2c41b366f29b17104bc4cea98afbae0d7af267d2207c17940408600f139fe9d91d4b8f5d7080fa5172427723fd8d2723ddab12d68607eb110c1f37fc2e052aecb7daa7bd6b632a2399072ad47e36d13896859c5dff87454a8d8866eaa38cce63e240a7431f569bf7c2ccc2094ad515748646008f64702b40e9687876bb3152ad28c9112a41e6b506d742dfd205f9d4b76f198e7c2f36c787373743b322ee09a9ceba938facf376d4f60193656869a86f7329e3221c986c1df64890278b26802e745c7751cd9fa093358f8f1bd2515b0dff961ccb9885c81d37da8f737683b80c307b3dbdf0714e633ecc90d21f9ea1137bd01724e21cb96f23c1aeca7946a4a2c882384c05270e031884858ca8cf5e26640716cb75bfaccfba8eeb128d48a04af1a10a169ac3d137804f7e632351efdfcf536a8286784e7b1a0358b78015b64a04714da87057a26311e36391b94aaae7ff7466ea0c6cd4991369c34fa38c4dd875ff402de7f247e57ca3029b12869adf67e673e6c72d4dd5345ad2b1e5b0933ef49f5eb33e3943120d8ff10481264b3c0a53367b3580a5e0b86a880d6fbc472b44658796f6e5ab10e006246de7cb500b187fc2bc475c5b7c658410e8ca6fbd09bb4e1b339f1983405",
        excess_hex: "32bcea351cffbf8e69e8c230dbc07b1579292720bf5b4e86f34752c5fd8d9801",
        sig_s_hex: "9843a95b8a9f74041c7380134acda248a7fb3feee67a961a10fe44058d45980a",
        sig_e_hex: "dc3895191e889629b281e2e2898251cd2c7a51a8e8c6e59f0c3ad7ab7636f302",
    },
    LockedOutputData {
        value: TEAM_TRANCHE_VALUE,
        commitment_hex: "e828911b58643d6776d02b33c5d5e3cb2ffa40b97a19537bc043fc6e350b1a63",
        proof_hex: "c0f5897e79f98f177a92a0dc903f0c0bf3aee5743edea9a96572f6b9f1f120530ab68236b7fef3ebe3c1d03b7c658c4b711ec5fc67a7823624e2e1ec4b70ba18c2e95195f657670dff436043f83d3e2b068c7c55dc8f240cc6a11f0b5ce3107caa1ec0df8ad3eca490b1037c484e461e2687a80beefa22f8450179115c9b0e6f6d627382b142ab93826ae52bb9243c6d1494e1d431979c8504d8e17bee350b0741a44a6a4fc6fea456f53153d60bb19fc299031cf6f7eb41434286f26442130a2895db4b8b3f4b9ef506cfd6388bf8e7b447d4f0be1e66a58e74d9cbc3e7890768a6a5dcce9cc903d8242824113636e96fa6ad22166872691f2b10c83e843972f8d5f37e16d286e109c1d860572000d7f94a512fb9df7944f55e9a68f9d5841a383f9d0487140d698b268f202090fd0c0db28cffe9508bde52c19e9bc0137f69548073bac8dca839c9d49b2a0bd4c963bc40933dbddbaee494db5c5212700d3314f7394c5419434b6ed61a44c296b7a9bb628da523b1bd5c50ebbcbf0bba432692c2bb6b78d9708cc81c648c70ec4c5e7623ac99a93e8cc5e51defdbf699df09aeea705162170cca901909bd3ab8042a19eb7c0faa26cbc92dc9e84049603e0aa6abe402f2fe820d72f81e6f27bf70b78fe328c3edb8c65e6a9ccad1c39cd5220e8dc2ab73efa585b9da7b344d1d7ac1155918c47502ada16a7fae4039d5b62a60dde0c08749fa7e4fb0bc0c331d6a139758e51e8da7a08b5707537dd644b97dfc111fe45a965d2cca52c2eea6631abf6758aca9439bfd0ab59fa87039e3ee13fa70106b2123ee695135b6ee5cb732b4041b67e047a0ec95b483cb149e1ab56c07406ee7d895ed5cea3c55727adbcccf634d136f53f841a5a6368934e405c9074c0ae7e0025cd952dadc610a704d0873dc3d62b00150ba78c8cf18e51706b700",
        excess_hex: "eaab9c61cd2cba4065a122350ee77531f4d405b43af08776bea555abd1741b54",
        sig_s_hex: "dd036479df852db71b4be764de507e0505c176436e002e38822bc4c411cee90d",
        sig_e_hex: "e0d64c8d7c1d86f4d8bda7d7b7582bf41c6b4a21a0c2e5d6b072189f60c1680c",
    },
    LockedOutputData {
        value: TEAM_TRANCHE_VALUE,
        commitment_hex: "c8f1b3124efdcd72ff844deb787d513854f76005101768c45ab14b9d55023725",
        proof_hex: "1a3d2be9396fda85466adb57154cb245b91c46825377806514023ea0b86a873ea22c2edff6b73818d386dd4c7c578008c992796581c3e3fea4353a87e149063f8081159ad3b4821c4c3e0895c165629921a172545815f7994192968ac62a6f26c431917a3c30426b613d2ecd034cb91d60a694bb9a5f3a6acd7bce17e45a796bf913ffcfaf1b07fdc60d230309ffdab5c80f0d389d676a6386a229a8c940b702692e8c558e40664caf0c7ac921439ffd24d0845604f75edec9b808973755c909fa6bbad1f8a141505b0d12c375cc04d70a905df12608815bb475b2b02de4370db6d7e373422dd46228d4014c4f6504df5297102835cace48a809396b46a2645ba6b37ee5cb03e167ae5b1afb8d277a9050d1297bf4f73218bff6b789d2f5113ae42b08f2a03bf7adc921587b836ed963fab11aa5843a3f9e3b8f5203def83f120c760b5de30a8cc2ed3bb96e22d9c5e2f06a287066722938cc4c89fbae2d7b1d94fcefeaaa7f35282afb6045485b456847216e2bccb35fbf096eb362ef847c095005d5652942ab20184259316a36764c4ddf83ec27872a0cc0652d75090a4c13c6e10c1f7d06963bb6164088092372d1c850468e20f3fd197a4865902063a0001a9912a67396b9e36ffeb1398e2da5a4c3f3a5f59d13348cd47871b551dba0162623b70f3001fa7929ede8f5f81cb8959d3d1c6ecb11b9c1eeac1d1126c19542308fd0e9d6c386015fa76d524b5c278d54d1295a1ecad92a11432dfdd8c359005a0697e5e90b2b8b53eb25f3060e2feeadf2d285b2388fb4166b6af4fd1291147844ab2619902032d12a6fc2d53275ae55daca4c22d04cf95ee147a3cc55323dfbb3f449ae71c51772294830aa53f0e1ffeda01500a0f32763a95e5d9af6e40767facda7954251a6602f38d21581ae8450f3403a748ec82059e9470eed98fd0f",
        excess_hex: "d01a306884ce32c2ccfdb90f99529c1684421b12f61131d6a766f58efd825404",
        sig_s_hex: "7dba1c918eb59ae24bff35e926c357cfae4f534911b65c95c22246886c94f80b",
        sig_e_hex: "63d2a0e2c9259253aae25bb13226b2146727f02614d36a8ca427d8eaba0b7f09",
    },
    LockedOutputData {
        value: TEAM_TRANCHE_VALUE,
        commitment_hex: "32ef1be1abb44cf40ced0c411a07f7ec0be92a07abd0df97e4e9190f783bf829",
        proof_hex: "ccdad02ab1761f1068eecb2e986890c80499678ba456e8e9b17e9bff72c0222424d3ddca511e83d88d36a42b6f4f3fb98fa4d98fdf0b8032badf0d11beda857d3cd7e75e8ec62e3317b2cfa72f6ec4ccc6ab1ec1a8a3cdc26e3e10f1c55dcb5f70bec6d0fffd546f26af0ca8ec13f819a7b62853b7d47b7924d6433c9bf78809744364d7a6ea8d726aff3f470a2776a208935d41df5c2664ce447f3e2d7f950f38b3ba0076a660ba1ecc4074727dfd0ec2ba7d4c2c0b949791761fe402b4c705840e2878842f28e681b326a2d352842119e3d76f46d76843852b4ffba8929c0a989bd4cf7479ca4985183f677f1cd33383cb5d29ab728fdd8b56c73ffccf0b601c3aa7e974e678ea46f0998f73221029b801fff718c8be46f5d30d736b43395c3ae0118c7e8361c23329637c3dfed5a2e83458bf88b91c191296d6eae9b5d30990b92c3f6c968d0420adcec36d3e9e1f769268e0b7a00f02f2dafae219b92434506027b8f9b501beacaf712b64fa5a5214b30e586db7083a917f1dac0d8f9879625819de13f15461ca0d56a876905ac0836847e841ca868bc3512229b001cd0c4438dd307e21e582e1521deb738ff0c7c00b612bd5e449d8427a10a6149b0b4e9ce2143b7963492854235642a22db03b83232ddb8f0a5191e1dd33198df1d72cc060673a4544a30a97186c445eaddeae4097ea71faae23ce250c8854eb6ecc3fd613b4cdfe8f9841aecb8ab2c998c1866e3ad68f12350279ce2a6c78174de552c48ae8963c3caec1b08ca06b704261d8aa4751a13c7739b88f20d82d34b73324b460721a518ba25e8313fb4682bdb924cc98160a19bf9491c97a1ac291cc29592df719622218fd211ee6c6e37db1e1de19592afe4311880c972e51d9c63b42057cf1518bd8e64e2343fff6fbf2511cc9b14622fdf32ee2f5018a8969ffe08705",
        excess_hex: "6efd599e39e6865c7d70a1fb17e90434953376476bab6be136aafba29ac2bf35",
        sig_s_hex: "d37cc367417bd10c8d152a85d57d2e30804a3967c169b58a19829749f3e6770f",
        sig_e_hex: "bf8b054b7e26191e9b56c917cf3e1819bc9a4d311d117f5a5aa7219672ae7b02",
    },
    LockedOutputData {
        value: TEAM_TRANCHE_VALUE,
        commitment_hex: "7e7faefc01c6cdbe33ea9c11a6a115a0128be36e7ff9dd842fa7651ab9c2d715",
        proof_hex: "d6afa971beea396697b7f1b14472c78c7dbfbc249856edf481efcbde4236bd5af04739a75a5765ca05d2d3447ad0089aea15c7aa5b57c7a4eaf6a3fc42204e52acc394bf6588ddcdb8847e8880bf34750418b062cd51043be97528c54b13b73a3648ed70c7c97175bf48eb3c50405c7258688828f73683e6938eb87299daeb1f99e270756a43d067795527df64b23ffe1cb46e3228c1cb70fdbac2f4e0bd9e04a16bb5126611bbec5f1e59cccc06923c749de97ee6c608e22eb9de51e513da0f33bfa358d26474f52b09663d11935aeb9ea62e3a2244faed2d1013598a33320df06f30095a23d7fcdce92da40c52f57a36543fed3ed388a318ad341c3df65c3b64e81deb9ee3d26948b0425da41dab129ec9b404c2e7eabb14cf45f133460e331233cf70945c98887bebaa861dc9b3a0007c9c33d3f09303f943f011823bb307b48ccbd28a8d9558f603668fc167081bc50b11f6750598a0a19d30b8417bf119e2bf01d2381e69ef1ac561033ce7ed211dcca6bf175740b5de520524501c2d6c92e7d5bedf438c5371805263c7edd7c5518f588e2231cd64754c5f12b1fab721162a0ea5cb6922d3339f3723e252b4af31998515a1cfe3e1332926195071245c2657123be7a7cac748cafbde7410461403907b7625fe161f3a998cd1ff7cfb13107c199d1e37a082912519809bcbbdb0593dca3bf381027ccc37cadff39310403808ab8b7474ac6b434b8d9f1ae1985b63a7bf51a06f0431021becc085d7ed78d2926f5ec1ea95bea85b849a6efc1809c7a21dbb76b2cb31ac4945aa17287148c48498f5074e5c6db4599e89e86765a58795850de1c85f93d75bdb475cc7a57b498e342a67cdad250eb98fbd3f7829b71b8eb035a4c1eb42cf17ecec64ca810bd0f94f7baf9024f0f03dcbb36768333d0a627bdb476bce94e6d092ca2c8c770b",
        excess_hex: "04ad83d2437d229a999d87e8ea6067b990fbfeee81e13f0b5051b9aa355c8931",
        sig_s_hex: "312ee0cfe9a207da64fdcc8f2e9c2f0132ce9f52306a9d8666972af5b01af706",
        sig_e_hex: "9ea89a00e741e63242aad4bf8339e69e10e757feb1125e477c87e3b8155f6d03",
    },
    LockedOutputData {
        value: TEAM_TRANCHE_VALUE,
        commitment_hex: "9ce2ec3c82bf204a8a3f07e86d1f979d2f24fbc1a093e440f47a5332e787f87a",
        proof_hex: "42f73ce52e33fc0abeb7df721ca9f58edd0c3bab675a108c5bdbf74a7a70ba230cd5628a403d840f2a1f15789ba037f0738ed1053e7c82d1ca3364907436d23d8c9e93c4b45e7c06fdb92ac3c77f82e970de1a209a92e42201155e294cf6c34946b05e809175d96081e5dbaf9246e04a202d85fdcc54729c0a4e6da07991b17fdb0816a7e95b43e651f247cd7ab8ddcffbacc01f997e322b1009f5900e5f090d1856dcfc1c13748fd4c822a362d391251d50cd139e841234946bb18e00b3010d4f31d4e9919cf5ae0df5efa3857a43b50ab7be1d5f15ddd1fced1cdc89c1c4059630ae486aa31125280cc1ad89c408e224e2ae1d00d701af64068c16f1f2f03c16ee2a4560bb83d36bc66a607675c60575d2f3393ca80936ee7ad6c89dfd620c5005f6bae8edaf08a77416701191378fa2fe982cb046a0292fe968ab782f1b113eb6663f34c2a74eb7a11adca0a3d4f70682c1dc695b27abddf9ac9f3fd3ce5702d65352ee502851b338c7c2f5120ee913471ef06329b6299a64956bf8011e29427ba1751f80057b57538f68740549875d33b89ae469ec974534fd913ec21f372e44120abe6fd7fad1ad8b2a8712fffe8eac7576ce8651159502416bf16a025fae20fe8b2ff7bf89536fd467d01ae9ab142f92f5d9835c3c831dc9badd15fa371c52eb151fedf7a03cc8310d2759d1ae5e623b2023323d4f4164a5057bb66e2012a303e76bc9ea1ae45a467db48559689021420a632b8a0627e1688d3ea1416d3a59978b2c56aaeb4ca00852c0b4130651ac661b28b10f3c63ce6ce8ec08c45a5617aaee2e7d29af7f96ea393127d710dd6bd18a357cc0e331de4d3b88f8461f833163d6e25c577d5f5ea4770fd494f787551731f768810456b1b7c452c4ba06c10e10123a9fede8e0a6e51bc85b2316565ba48d6186b123f01180a065716200",
        excess_hex: "c622cbf3f90c6e89a54a9d55de0971a530ed6007ed610677dfb5f3b7c712d471",
        sig_s_hex: "9b08876fd5a470a0882095fef906737341c1c8efc70a09931cce558483fb0505",
        sig_e_hex: "5e8694f3806bfec9ee44b6c22150c9cd0eb340556c6269ee1f23295c7117530e",
    },
];

/// Generated once via `cargo run --bin genesis_keygen`; the corresponding
/// secrets were handed off out-of-band and do not exist in this repo.
pub const INVESTOR_TRANCHES: [LockedOutputData; VESTING_TRANCHE_COUNT] = [
    LockedOutputData {
        value: INVESTOR_TRANCHE_VALUE,
        commitment_hex: "4687ba198a1eb1b8e7c6b538b6caa833470a9661de2297e298f9edf09761b617",
        proof_hex: "8c5d792ec68812982fcf059671ae4da6dd4d2264fb7280845dce75b91dd58f11ae68feb1035803e23ed3ff4fdf732321ecf4211b4a1f382fcecde210d87c9e1298c440ba279d62ba33771a49f8a6509bdcae492919e5f652e41791f86d79bd22a821fc5dff85a97adeb13bd65141729720d225b344144bb13a7eaf59d251d33f2a54c1da1deda8a38e5542c398827aec6d8f6407b43d77a745aded7d0ccf9a04aba6ca0fd8eadbb279b302195e9c2f58249664a87ce69743a8a68767a771140b399f766eda12c2ebe060d5f4419c95798282b1f39bf1a5b393692cb532880003b673d10fbeb85630a7e993ee612063ad6a12b439574c1d878cffa646be82d21facca0ad82c6a7e275ac5bfaab2b7df8f15e25da0d4a0367f4cfe473ebbf1401510b9dde447c806fb4cfd5adae7597f5d2bd50cec0771134977039a5a9b99c45380baf2240610dee1ffaedc0de94192108e13f1ce75e07f5d39c7394c8882f3388eb69a96a58591b44dd59c80c8a935a373b9451903eb16d72d5ab39e3dd7463c7ec2e69c3300be09d622b309e892a42c34e6e77b964f5eb2e04c5aeb8b7e5129bc4357114f6b199919504c77eaeb7ca002c2980c242cd3d2c442cb3bd3335c23fa3bc64fa869181ec0051bb9fdb542b5d36669ab9c4c0ae04aed811bef709c6dfa7d5e1fdac273c16bfcd35b63b6fd81ee5cf038a964a7b88dbb97e29b14db1760ea503da7482e61a302fd2ecebc1c8192bfddc735769760db6b74ac833d776d58f98f3efe8c29c2f152386c91d42b1800f65ba96ee5a133142e203d69656f06848a58039cd630ec90a639c40ca1bfdc4ed28667e5e62395cfdff4b1bd43f67837adfcb9ae6f58728700da3e5b387325d8c2a5c3456a6c8fd6301b715200a50e9be7736f9a6b91033501d8a74df51207ebf4ff8af07e6bef513a6845fa9b8b04",
        excess_hex: "267b0d93df5f1c32f8761b698aa4236e725e68f90e0832e2ab2b0e5bc58bf034",
        sig_s_hex: "3ae1678c0232923d134abdb46febe184e08d2c6e8f4b6c4fb79577bee078da07",
        sig_e_hex: "a513cf4826ef74c1637c317ecf7c1f4b22e0f6843fa0ce01f25595cc6815670c",
    },
    LockedOutputData {
        value: INVESTOR_TRANCHE_VALUE,
        commitment_hex: "a083b9609f7bb0b17e35df70c64a474ee4a283ba821b5728eeb1efb5218a626e",
        proof_hex: "40aade92e688e86ba904566e035aecf22dd9df88297907751c407aa9719cd13a0a2873055252b87ce57442dfb1e990e2c4a91837aec8e9f67c8d24e92dbc6367acdffbbbc2b7a8294214fed735e855516f076eed6cc867b827054b40f24cff717027ccee9d30e696e7c41daad5fab7c7a9696f6df7dcd36d80d4a30ded7b360701efc022fbd67d2d3a7723545d942fae985e0e3a2fe64ba5b3a318b39310c40b15e7a42e2260c17ec492f2c18b5593e1d6c21a78db94a30ec6acb7aeaa729d075a2a33e8d6819f79901fdbcffaa6743507a32a0a332f5824cc0e3b987437db02dc7217966e23b0afe13097f32f83204af594e3764f88e87ae35574e99106777b0018a65101b24ad5fb13cbe6e220e741f6302bdc4169ca663dd4e63d801ca71ba6b9a6d3549eec5520d8fdd867a075bc3753ec8f716ecc7f153119c81ce307031e335c6796aa4d5a016a1ffcf0acedce6df6f85a6c5f36fc6d2967923b8f310d0034aa1532c84358905de036426eab55176e6587a7ea3bbbf6771034e6a83e709a9a0cfa12d7eb6b0b61869c81f743d8f5485b6f6bfbc4b88f13543c62a1490bb2f34c2199f0798bdb4b984ae844d2548c6edfc475499feb2cbc9400e0bead59141495f889399033ed071ecf60bce15c5d10395099ec933ad5fa81c16c05207cda80c0dcf54c39c1d6693f3888f2e46f25cf67c6c1b1355ee4ecdcdebb58a4725cb632fd4771c18a118795d63b74f6cc4532e446a708182141d8a53a32250d097490639d70d88139f517d75a571b317ec1fbd9ebce6437fe356e7a6d3bdb431d72aa67a36e06852705ba2aadf7c7b4876865e14b92754c84960b21a448d01e004ef0f9576a83579e28ca84b02fb93d31c591ab93a5659b1446d55bde93cecc0415936287b60012e51d4476ef6be76fece9e0865c5682b71f6cb6a6b72f1e9f07",
        excess_hex: "0258287ae2b80af0cfbdd788e9e6608240d4695f4dfbb5f71b9e6731ee4f2561",
        sig_s_hex: "7836481f6cae98a425fc8b0e424ad5e54c66a4962c2b712b0ab4831971a82e06",
        sig_e_hex: "2eff3d9b99f93a3b4d5f0d216677c7832997fa9993972cde21725d7b02cc9c08",
    },
    LockedOutputData {
        value: INVESTOR_TRANCHE_VALUE,
        commitment_hex: "4e97b53d34a57228e5d07bc7502f7ab22afee110c4be12586d9b683e6535eb0e",
        proof_hex: "4e2ebf920cdb6d28a1c22771a4d5c47f152cebd51688a6bbeabac6b5bf780a6e2ecadbe5e243fa4ed83c8858dd907dae8241437a4d8c12bf46576a7130816857440c7ab603f0a95948e5663fb9d135e275a9e21ba515da1df212391597ee2a209a98e5c897bd6af407e8d31f51bc6a0701b19dd2a972d03bfc1286e23ad5930996a85277c6137a36764c6435f16111ae74b7711121ba6ea2977175ed128e9a0bf9041aeac881a2b14bf7e7362955986e483d667f8f373fd8568efe7c5ab2cf014fddefaf933e1afabef02d001316325b56f36f3a7380ae8b922aaeecf00f950112d3b65bbe7e10931147799dfe270335d279e54d2c98b54771d39edaaf54e136e03dbce40c7d33b65c241c91d360da3eaa801c1ff17a504daa8fc413af26f32062c5d4996e457e44e007aa6a45805c68ca4d2cf05990a42189d241b9a183086da4aa268b94116e9c1deb1772e1d161a6d1957444ae4064be50bc072c4c6bba7bca1351d933a56a04ae889910750938777bf14c36c76f93eaae07d6ac505e534e8caec24b0cbb498fcc0015cefb77edd0eba4c78415591391aa72d9e8d205d40568a11087e74b5e202e525a53ac8a9d74239f64de696babaaedc1bf27e748170068eaf1a6076e3542ff8ce1b2e21a9874f45a9e1f943d643e8097a7d3a3651b123a43b944d72675db4b206e16824c6545c013c79e7bec70258cb08280620b9d0a4cf5c6974d773cbf5d0b4e166cd9227f89095d1ebc7fca805c3c9ea1f23f4a016085d5d731a276682726b34fa457242a8e0b19a8a57f99c12ee8e71e3c50117dde46600d90cacfb237232250760d25daa0ce9a0f57cbabad30efe7a2f8a1e13bf7450fd36b35900187b57531ee2a290e4c98c27cd97a2ac79a0baf0c8cab8501241f3c8747aa06eec15afe562c4e5e8fd7b1b8a01489744eccd5d1fdc78eef0b",
        excess_hex: "541e0d8541ecd224bcf3ad0095d3c8f0403bc869006d2856bca9daafaeae316a",
        sig_s_hex: "f7d8361681e2778abe1ac65f539e168dfeeeffab2194dcf2b7d322c28abe700b",
        sig_e_hex: "335950f3d6a8820b8a3c945c44fffccc7a72f929b8b03d8fdbdbe04921094308",
    },
    LockedOutputData {
        value: INVESTOR_TRANCHE_VALUE,
        commitment_hex: "320791264f0e461ed0f1cc982ef2342c548b6916204338ffcdbfd14356c9d95e",
        proof_hex: "b04f1b8b511973cdc8382ffaa5761a624a27dedf76d2d1ac34b96b0184e8b8165e39648e6bc30729c1729366d2322ec61a838e65e53751d2616a1679dcdbe665f6f169f640b6985846daf4f59cb29febcc3a88beb14956480b24a1645ee46a0022ba201cb4763bab8e62039d3520e1c992fdaf0d11c756b5aa67eb5708e2926f3cd3abaa876f7a0f2b23488626b694fdf078c53c3a417be42977c49c7a89b10e015f8d5251d64aaf3de81de771ae31a6274e0280c42fc606d6f26096b492950706100651420dbaec1752bda4995413f919a31386e39097870267d9ae154d040dbae30360ff2a5fe305c8e6b1cb5e40527f5c570d9d90ebd7e6381c693ecc7b5ea2088ec450774363643ed367b253b8b39ff8607c9dcac5f0df851437e2e99e0c5a7939d1d83be90024c194acfff363578e1c4661045cb9c2db539bc6bc67f1384a65935890a9488da637c425b17e76f07b68cae39065473c1e84b33d6871500c7cc5cff561ca259f63c3d5d51d6c7d11269d583ddb20a0e49280abe7b9e0db3a960d00311a1b6b8b254d9b45b8aefa4b008ee013a697481ce5c0e08c995fd6426c9d0c33132e48c3cac496eea4698b36ff05c425e23500c22baaf4a052bf257b1ed6114f0e9de39bc421852b602e99a151373491b4061f1dfac82eb44165f83e2067720fd417c0e821b33cfbe665cea8f77cc56da767309ecaac94a9b10b3a57700e4c1eb609801eb03f8379a020dbdd41f75b6547c5f94084c822a7c607a44e70e9996071944f4e8d9649d840405de83f70ffd9cfdac3ac42e9186128688365c29ce913b5aa5c9efaf1ea83257bc80518ebef60ec4199e4e8f03c74b38bd974ac58f07372309ac24071e91943065563df9bcd1dcf6f65073bb98c88e1e1200fd2439eb497dbb8c6a520e914702c8010e9d38c0e9031715ab180179439d08200",
        excess_hex: "ec49c61412e197f42bcfeed2afa7714785aaf835b9e38eab682fb406f8561f08",
        sig_s_hex: "24e68dc7925eaae1e4b46f5d466e9115dc0814cf6ce547dfce153eb87d71c408",
        sig_e_hex: "a105fa4555d9c20c4228fb74bc8f2ed42cf238f02433ad3bab79f0c569c98b00",
    },
    LockedOutputData {
        value: INVESTOR_TRANCHE_VALUE,
        commitment_hex: "3a99f0ea51a406abfd06d181d451947183398b40c0648c58b4b8fd70599c4752",
        proof_hex: "7ef94fdfb7685b23dcfec3309530858f0f980438b97645a9ec0e0af75eff4273242f5a780cc8226b5a73c5bf2748522df850bfe13027adef017773537cc8486770e8f3ee290b2a4612dacd255d12141eda794bb1c45cafd765f9d0ad758af549f40fdddb40770cd4cff7489cdc8fefb0081780224b7a4795b2da9ae5f8f6b84ea83eb8e03d4c680d110deacde1fcf98dab6ff38476ae91e306688d65bbbf930365f3a984752348aa7650d3ecc080faed61bf5f3c1eb549b1e197a946cdf52f0dc8030a4b8ca56ce614c10a7f4459c767bdac887c3113498a1256b24732e6390a28dba37add595e13f6902769ab38e5af0aa02537c53fff51328c6c44f4541e35825d7cbdf614b41ecf9a030b99e8ad14f7aaaf3d9c20e77ac1062a21816db502a81557d9a749bc6f95af26891a65142bfb4d6581dbdb284c55aecc47b7c6a771d8759140e868bb4ada4d07914e4c557b0483b81e80a6d08cb8f0e6dc6edac648acef5c32af340e25a35119adc006b5d21841c6db2b326dfb937d1138cfc07e73ee98cd82e008378f56b515848c49aae7db72e85dd8daca14710a018ef5f31a5a6225699f4dcc630c81868379ec671a6c98870976de854d0e298327f7ffc9a14496faf9df7b8ac420858dd21d4fd935a4580bbc9841f815268f6ff7dcfa922d634ec0e3efc7642529ad889b65d996d6593d52480d6cf0278f15b82d5dabe5884ce82705a3ce052d9ea62357287795a662f301a94795696ddfba0c07342887e913a05617f74405346ebda5f456e039039e16dc26a70b1dd3347253a0d70ec93e621458e0035cb7b3586cf924f63213e6c79112130057c626c1baf61f41cd7f321c6e160ead7859e8c260a7a43f599d95082fff4bc576d14d251bf8894f8166620302ee56bdc5616a2337c56418dbdf1cb8847c721b2cded7c4828cac057499b205",
        excess_hex: "740973aef3d5fefd8da1b421005503b386ac3e18d1295d4268c0a55d262e9a48",
        sig_s_hex: "11e87267442b043867fa327f05d27e3bdfdb74f5ad42a2cb7264742960a7c90b",
        sig_e_hex: "39698bcf7a6037ab791b32f32c9b64e54c595bad5756295aaa70acc00b4f6009",
    },
    LockedOutputData {
        value: INVESTOR_TRANCHE_VALUE,
        commitment_hex: "f065853e51e1749e2f2706e824f6349ca9dad8e2defd8694ddbfe7caef39c850",
        proof_hex: "c04ffaa13b9a2f13140f032cab83af5d76950e36427966ebb59f65a4447fb75ee4709ab47ab9fa87033eed089acd8429adc083ca06da8ce7b351ffa4eacdaf493e8ac9d2b01eb321e586758dbf36f7785db7c9885b111f1d42c578b71c2a730f180b2092e0afc285d19139312a9e821ef5b45080283102e268d8e557cc3db2333daa22c124923396507e2a9d2885fe4982dc9f949d736cbadf2e1e11bf632f01fa54fc94ba3aebe4273cac136c4d2ffab7e9fbd756787eb48546cc690c0ded0b09f55c8d52fe3ad6a752338813e883c5085c9aed0069b7b2601f99e52a249506f2e23f32f8984953c823e427587bfecb64980371053d8b4bdfd5fa317c6d28178c9040d0bbf13bc669fd93dba3b814ff74f5340a73dd3d935d1258b7cb7bc1169af5b600c8134a7c2362fec66e5d518d8ba7d2950032a97225c39f5636c1c906b29acbfe49130335ec38b549999943f662c00fe47053fc8483c2f1fd987f7a09584ea0c93523237bc29993004b929181879513f743b178a6fba6441f887ae6768e45d37db624a86a11479389491ca31b006d46dbfc12f6098891782626bbcb17629969e479e1a418f17f1639998ff44f637b7c43a7f1803320d62ac89a51122bd27c941386c5816cac2f7402081cee290209d69773362f2e56bb09fe43fbab206e8dbbf1c9390daea1fa866b669611bfbfd419251c23c4ebdabbbe6f10ad790fe6fccae9120c70c4fba387c37f59be23558666b0b192fd62e2ec0e794ce1d40d90d8a4f17ef4e0938d267eaec18da3a3c2302f5b46f7f76a1d53ae642070351acc47854cc444402d26d702a631b2a52411cbad3eac86e0271051df8e69d40246c202d23e8aeb09d122714b1de25aa7c24426b165a0e6dbdc1d15d5013563160bdec1b8a12f6a69bab211161ee71b6cb388c2247028f65cdd35b5b9afbbdd3502",
        excess_hex: "561413636bdfb39f343ce8c3f6daee2b2717ad1fa59096a78389e0de70b2af2a",
        sig_s_hex: "960fc7bb2c423d982b15512d87f208982d2f59b6d37aae0f508d801474c86d01",
        sig_e_hex: "aab68f18b239775b4ecfc29d1aeb80553ad9210596b436d85610b851912b9e0d",
    },
    LockedOutputData {
        value: INVESTOR_TRANCHE_VALUE,
        commitment_hex: "3a299f6de316b39403beceadaf188de0b5b383d2aa8d99177564aad8338d3241",
        proof_hex: "4c14af7d759ececf1516e08f9f36b14f01f9624eec9a87ed4cceec9ecab4167e2eac82f6b965a20ed77068d9df5569dcafd37b70a068cd1b59b709687547831e0c61c9699100a3e495994c92b5a2948095b0d6e5b2b2f59ab75eb2f5b7f7e472580494cc4b5850beed207378680256cf541c353cd9f364dc8429ae2dc24ee77d84a919b3266de28c1ec61165117bf81f00c55b4525d0a593d9384b4fc2725e008461ce0ac987c407e4ad8163b999ab18e9a8596aa7ba643f869b6aca399910017ea43b539a8d2bbef3ec03880c27bcb3bd18e39d023676b099d800bf669e750b165e8bd427473737604f5ad413a9113a33d6cd533980f3b4feb4d2559191fe68b4801e7c21bd85a8e2aaa2edbfd76375cb8fb435ed9c25e3f64006928924731f4edd9c6e3b981ae877a689c52e21026d63a4e49bfb5379df8bfcadac4923496fae0b2dd91398f9bab06f2904576d0cb345541f9235cac8fefa685021895b2b7e744dc748c0c251d3bdb95eee050292faa9b16c431e04796df51500242f9ad6212ad56aad67c8fe5baf276555010c5acd459581ef016e66c6e65036f0036dc70454663af34ae13c83218e635ac70238ad7baeeb27a2c07e353fb0151e905b1874ec6464aab66252af92b0c503396132779c7ea0b6ba21becfc2e9135f25d33d756a620a7130f6be5c08f59f7757a0d28b4c97d8e84cc07371f9be174b789337316a889f0d3db1b056388eb1a59722f4513ccfc96854f4d148f315e6278d739e007026cb978d1d9f54646218c8fc69977a361b8081c093a8a763b6f00932546b5dc8816a91af130f66f7cd73ea35e34b79a0f20ddecf35e0c3edcf7fa69a89b068917881b95a726c62fbe088b351e3cc3507b3e0bc8beb97785cbdeaa1c1672c009421a09a2ec0146d6581f38e6edeb145a3b77dfd7d97f71af659b65f26347704",
        excess_hex: "b03c496d331d176549fe7e6b77f0e4f5cbc1c61d47d594899078e40912e7ab2e",
        sig_s_hex: "915ebbed8f4cdc7d75f05dccd2e5b57a5a9d973f92f6b70acd6d6c84bf803b0b",
        sig_e_hex: "df6da7e5658addcf043a18cf3c0abcceb09800f2fd1692397d53c657f8485d0d",
    },
];

/// Generated once via `cargo run --bin genesis_keygen`; the corresponding
/// secret was handed off out-of-band and does not exist in this repo.
pub const AIRDROP_OUTPUT: LockedOutputData = LockedOutputData {
    value: AIRDROP_ALLOCATION,
    commitment_hex: "c2f992f07a6fb0820065dd2b390012996a83b2c56397fe77a6624f6282d5e906",
    proof_hex: "d0c853a046656e27f493e5d18bdff9645d662760e08ebe9f3460ec449eb6247e30dc3329658df34a4ad29669cee24adbeb6c481b84df1fe4b3ac60374b12866f9007b19d083e2543f4c60d6ee550a3e2e4fa264c8cff026823c5017f804c60121e7f43b251e7d2b94f286dc4a623a7148a45b898d06c4a8182e57eb8667cef5daad75a1a9769a52dd4a81265cb1c9d9744f0a93087dedc0d876c151bf8506a0b0d74b89e5bd8ce1e86620a49d573b3014ef4884754f691bd725724948a07c0063d0f9176b695e44f09827d090c8cee878642f2556ad23aa47df20bf75a7ec602e83f02e73901b71d1cd2953d242e1c0f0ec84e753fbd4e7687b41c078a386d6ebcf881db095d79e7b0ce4fbd7ee3427c46954849f3a6301c73cffde87ce47779367e6c1b756382dfbc6b43f0d94f949e4e9d8a898b3760dee6f6da09ebd8130590234a7cbb2817c6b1fc585cbae361bff1626aa238ec573a96e0e0bc26698808a02e37719563c4f5444dc237f7465a619dcc775988731dd94dd25f6206e08727ec56046884078c2a5856c6b77d8f4b33f845f6b30fd4f079737a6fa6c96cc824caeb25eb792106ddfccae2d9baa04a1a66e68e4c17f1a16ee821f34e4858f65cce5044e09477ed335e853a9ec6df36dfc6e8fd4ee3f180d1b05d7ad91e070672602b2ff76b2ec83d94707bc3d75dc5bd57a3bf79211836fc8fc4e5307139634bce374c08e9176d29953e54b09152e0e52cb651b8fbe9260c4adf6a2efe905365864da134e391d9bb89644da33a112cff3c2f5e42544dc7ec9b403dea9a80e940c43136bd9eac9fc563d77a1ab1db62df0a56e029898bc09209a8e14c0f2dd77d5de769ca92fd7b1cebd1d82b948ec4cb7473732fbe2cb34965b41e636a10d303c9fb9ae49b8c2208ca5579487492420e58f258204dd54c4ebf00ef6b3970720b",
    excess_hex: "c4a004cf0b2d9fc46bfcfb7622a242c2382a6252051392c4098888f4617e5f12",
    sig_s_hex: "0cc44a27e1ff423315b7adbaf37f63acf8dced871fc772f66c5378795c136101",
    sig_e_hex: "582aa2799be061479dd79d4364fce88933af6ccf299f097d6252876d99a85e0a",
};

/// Generated once via `cargo run --bin genesis_keygen`; the corresponding
/// secret was handed off out-of-band and does not exist in this repo. The
/// devnet faucet (src/api/faucet.rs) needs this same secret at runtime to
/// actually spend from this output - see wallet::planner::blinding_for and
/// the HAZE_TREASURY_BLINDING env var.
pub const TREASURY_OUTPUT: LockedOutputData = LockedOutputData {
    value: TREASURY_ALLOCATION,
    commitment_hex: "9e3a39b1fae60d91e5f0f2311bd26208e27fd84f5b675b5630054abf61d0a201",
    proof_hex: "e8869b7c657367701249e255e498c7e1b565fb4c8ff4e94eb0c380c87631275a8410043b3d286ecad7e5010f2c2b4cadf4e7f4ec95735001597a83bdc671630612e1952d6fbd8923d009714e63dfffe93f55db63ec1c2b16663cde63d276f37c62daf354377723bb9331f15bb217349b0271b3657743664c35c3fde0d871fa213a0f7c66606636f0b1c776af7718d873bbecf26ed6d5296c71cf1675f0e80b01b6b20abd47b6586afd4d128895259954c1653b068888a7a452ca2a2aaa4a2a04c1795439dff4d1435a2a9fd0a4c6054d50a7236d9d1424e7727b730e71bc170f92203c8a049a8c27410081c8d5511dd26b7d9e8548ec946acaea31444373cb212e592a7f048288cf04ee1667d3d5520d4b79d9abf5795f6168164e062370295cc29ef87e64af05dc31753d7fe7fd5eb649c6b52ff1d24502103fb339336ddc09ca44261f185dea6ddcd178a3c9439c82f20489b5ecca30b30b811bedba15cd4f96f852bf235db8b47bddac239aea81b2573a517f0c11b25a1a3bd3297059f91bbe402fdc363caaa7be4b7007b9baf06eb5db1ae7a2be8c94a3787d22c7102f2fcec0e1d544177e6e6debbaeb69ea62319782c8bea48cf950eb0ac035851f0c366a1312efe41d9f89a73c44133cd86454cc43b5779437ba22ea3bd5acce46294eb23786b2be79df5e8cc0382960db75e09a8e6a2136798f541fbda923df778d0fa4f23ea4b71d1047a174d05771e856fa062aae31c75330695c486b15d7c8b01802530e6a5dd9466bada5a4c9d3c4ad914f987876ced227fde5eed7992c16706f40ed6cc5d220733705a2864956303d68a1b7e82ec1feef1a8da29a323a8cb75b0a36a14a0382ef8cbc3d08f1b79fdb4cdee09164f3abd3f7d637b145cb5d270ceb7d2bde9b68897e26c85b74ec14951f741b9c8cae5284877c3824e692326004",
    excess_hex: "c0d6f03666d36f7f961ed0363ad3bb1324e3f5444c2324bc9dc634fa83eaf202",
    sig_s_hex: "9601dc3efa598509d1ca0d8cf2d3743230338123bcfaa7112f5a3e9501e1460a",
    sig_e_hex: "d55dd2f7bbcbc6aa3de8dfbcdc75e5e63cdafca013e5ceb333e3a6e22a29c600",
};

/// Computes and returns the hardcoded Genesis block for Haze. Mints 17
/// known outputs: the validator stake / claim-genesis output (1,000,000,
/// blinding=42, unchanged since before the tokenomics lock - the only
/// intentionally-public secret here), 7 team vesting tranches, 7 investor
/// vesting tranches (see core::vesting for the timelock enforcing when each
/// can be spent), and the airdrop/treasury allocations - all four backed by
/// real, privately-held secrets (see the module doc comment).
pub fn genesis_block() -> Block {
    let genesis_val = 1_000_000u64;
    let genesis_blinding = Scalar::from(42u64);

    let validator_commitment = Commitment::new(genesis_val, genesis_blinding);
    let validator_proof = RangeProof::prove(genesis_val, &genesis_blinding);
    let validator_output = Output { commitment: validator_commitment, proof: validator_proof, note: vec![] };
    let validator_excess_blinding = Scalar::zero() - genesis_blinding;
    let validator_kernel = TxKernel {
        excess: Commitment::new(0, validator_excess_blinding),
        fee: 0,
        signature: Signature::sign(&0u64.to_le_bytes(), &validator_excess_blinding),
    };

    let mut outputs = vec![validator_output];
    let mut kernels = vec![validator_kernel];

    for data in &TEAM_TRANCHES {
        let (output, kernel) = build_locked_output(data);
        outputs.push(output);
        kernels.push(kernel);
    }
    for data in &INVESTOR_TRANCHES {
        let (output, kernel) = build_locked_output(data);
        outputs.push(output);
        kernels.push(kernel);
    }

    let (airdrop_output, airdrop_kernel) = build_locked_output(&AIRDROP_OUTPUT);
    let (treasury_output, treasury_kernel) = build_locked_output(&TREASURY_OUTPUT);
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
            validator_commitment,
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: super::registry::compute_registry_root(&std::collections::HashMap::new()),
            chain_id: CHAIN_ID,
            asset_registry_root: super::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
        },
        body,
        name_ops: vec![],
        transfer_ops: vec![],
        mint_ops: vec![],
        transfer_asset_ops: vec![],
    }
}
