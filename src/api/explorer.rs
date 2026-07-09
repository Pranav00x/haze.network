use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use serde::Serialize;

use crate::core::chain::ChainState;
use crate::core::mempool::Mempool;
use crate::core::block::Block;
use crate::core::compaction::BlockPruneMeta;

/// Encodes bytes as a lowercase hex string. Kept local to avoid pulling in a crate
/// for something this small.
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn commitment_hex(c: &crate::crypto::pedersen::Commitment) -> String {
    c.to_hex()
}

#[derive(Serialize)]
pub struct StatusInfo {
    pub height: u64,
    pub tip_hash: String,
    pub active_validators: usize,
    pub mempool_size: usize,
}

#[derive(Serialize)]
pub struct BlockSummary {
    pub height: u64,
    pub hash: String,
    pub prev_hash: String,
    pub timestamp: u64,
    pub proposer: String,
    /// True original count, including anything since pruned via cut-through
    /// (see core::compaction) - not just what's currently retrievable.
    pub num_inputs: usize,
    pub num_outputs: usize,
    pub num_kernels: usize,
    /// How many of num_inputs/num_outputs above are no longer individually
    /// retrievable (pruned) - 0 for a block nothing has ever compacted.
    pub pruned_inputs: u32,
    pub pruned_outputs: u32,
}

#[derive(Serialize)]
pub struct KernelInfo {
    pub excess: String,
    pub fee: u64,
}

#[derive(Serialize)]
pub struct BlockDetail {
    pub height: u64,
    pub hash: String,
    pub prev_hash: String,
    pub timestamp: u64,
    pub nonce: u64,
    pub proposer: String,
    /// Commitment hex strings for whatever inputs/outputs are still
    /// individually retrievable, PLUS one placeholder string per pruned
    /// entry (see core::compaction) so the list's length still reflects the
    /// block's true original input/output count instead of silently looking
    /// smaller than it ever was.
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub kernels: Vec<KernelInfo>,
}

const PRUNED_PLACEHOLDER: &str = "<pruned via cut-through>";

#[derive(Serialize)]
pub struct ValidatorInfo {
    pub commitment: String,
    pub value: u64,
}

// Name registrations (core::registry::RegisterNameOp) carry their own
// fee-payment sub-transaction, separate from `block.body` - these helpers
// fold both in so the explorer shows the full picture instead of silently
// hiding registration transactions (and their kernels, which is what
// /v1/search and the "View on explorer" links look up).
fn all_kernels(block: &Block) -> Vec<&crate::core::transaction::TxKernel> {
    block.body.kernels.iter()
        .chain(block.name_ops.iter().flat_map(|op| op.fee_payment.kernels.iter()))
        .collect()
}
fn all_inputs(block: &Block) -> Vec<&crate::core::transaction::Input> {
    block.body.inputs.iter()
        .chain(block.name_ops.iter().flat_map(|op| op.fee_payment.inputs.iter()))
        .chain(block.mint_ops.iter().flat_map(|op| op.fee_payment.inputs.iter()))
        .collect()
}
/// Every output in a block, across the main body and every fee-paying op -
/// pub(crate) so api::faucet can reuse it to scan for its own past change
/// outputs on startup (see FaucetState::new).
pub(crate) fn all_outputs(block: &Block) -> Vec<&crate::core::transaction::Output> {
    block.body.outputs.iter()
        .chain(block.name_ops.iter().flat_map(|op| op.fee_payment.outputs.iter()))
        .chain(block.mint_ops.iter().flat_map(|op| op.fee_payment.outputs.iter()))
        .collect()
}

fn to_summary(block: &Block, prune_meta: Option<&BlockPruneMeta>) -> BlockSummary {
    let (pruned_inputs, pruned_outputs) = prune_meta.map(|m| (m.pruned_inputs, m.pruned_outputs)).unwrap_or((0, 0));
    BlockSummary {
        height: block.header.height,
        hash: to_hex(&block.header.hash()),
        prev_hash: to_hex(&block.header.prev_hash),
        timestamp: block.header.timestamp,
        proposer: commitment_hex(&block.header.validator_commitment),
        num_inputs: all_inputs(block).len() + pruned_inputs as usize,
        num_outputs: all_outputs(block).len() + pruned_outputs as usize,
        num_kernels: all_kernels(block).len(),
        pruned_inputs,
        pruned_outputs,
    }
}

fn to_detail(block: &Block, prune_meta: Option<&BlockPruneMeta>) -> BlockDetail {
    let (pruned_inputs, pruned_outputs) = prune_meta.map(|m| (m.pruned_inputs, m.pruned_outputs)).unwrap_or((0, 0));

    let mut inputs: Vec<String> = all_inputs(block).iter().map(|i| commitment_hex(&i.commitment)).collect();
    inputs.extend(std::iter::repeat(PRUNED_PLACEHOLDER.to_string()).take(pruned_inputs as usize));

    let mut outputs: Vec<String> = all_outputs(block).iter().map(|o| commitment_hex(&o.commitment)).collect();
    outputs.extend(std::iter::repeat(PRUNED_PLACEHOLDER.to_string()).take(pruned_outputs as usize));

    BlockDetail {
        height: block.header.height,
        hash: to_hex(&block.header.hash()),
        prev_hash: to_hex(&block.header.prev_hash),
        timestamp: block.header.timestamp,
        nonce: block.header.nonce,
        proposer: commitment_hex(&block.header.validator_commitment),
        inputs,
        outputs,
        kernels: all_kernels(block).iter().map(|k| KernelInfo {
            excess: commitment_hex(&k.excess),
            fee: k.fee,
        }).collect(),
    }
}

pub async fn handle_status(
    chain: Arc<Mutex<ChainState>>,
    mempool: Arc<Mutex<Mempool>>,
) -> Result<impl warp::Reply, Infallible> {
    let (height, tip_hash, active_validators) = {
        let c = chain.lock().unwrap();
        (c.current_height, c.last_block_hash, c.active_validators.len())
    };
    let mempool_size = { mempool.lock().unwrap().len() };

    let status = StatusInfo {
        height,
        tip_hash: to_hex(&tip_hash),
        active_validators,
        mempool_size,
    };
    Ok(warp::reply::json(&status))
}

#[derive(Serialize)]
pub struct FeeEstimate {
    pub suggested_fee: u64,
    pub min_fee: u64,
    /// Fee charged per 1,000 bytes of a transaction's serialized size (see
    /// core::mempool::FEE_PER_KB) - lets a wallet that has already built its
    /// real transaction (and knows its own byte size) compute the actual
    /// fee it needs to pay, rather than relying on `suggested_fee` alone,
    /// which is only calibrated against a reference single-input/single-
    /// output send and under-quotes anything larger.
    pub fee_per_kb: u64,
    pub mempool_size: usize,
    pub suggested_name_fee: u64,
    pub min_name_fee: u64,
    pub name_ops_size: usize,
    pub suggested_asset_fee: u64,
    pub min_asset_fee: u64,
    pub mint_ops_size: usize,
}

/// A wallet's actual source of truth for what fee to pay - see
/// Mempool::suggested_fee/suggested_name_fee. Fixed, size-based amounts
/// regardless of mempool backlog; wallets should call this instead of
/// hardcoding a flat fee so a change to the underlying constants doesn't
/// require a wallet update.
pub async fn handle_fee_estimate(
    mempool: Arc<Mutex<Mempool>>,
) -> Result<impl warp::Reply, Infallible> {
    let mp = mempool.lock().unwrap();
    let estimate = FeeEstimate {
        suggested_fee: mp.suggested_fee(),
        min_fee: crate::core::mempool::MIN_FEE,
        fee_per_kb: crate::core::mempool::FEE_PER_KB,
        mempool_size: mp.len(),
        suggested_name_fee: mp.suggested_name_fee(),
        min_name_fee: crate::core::registry::NAME_REGISTRATION_FEE,
        name_ops_size: mp.name_ops_len(),
        suggested_asset_fee: mp.suggested_asset_fee(),
        min_asset_fee: crate::core::assets::ASSET_MINT_FEE,
        mint_ops_size: mp.mint_ops_len(),
    };
    Ok(warp::reply::json(&estimate))
}

#[derive(Serialize)]
pub struct ScanOutputEntry {
    pub commitment_hex: String,
    pub note_hex: String,
}

/// Every output ever created on this chain that carries a recoverable note
/// (see wallet::note) - spent or unspent. A wallet restoring from a phrase
/// has no local record of which outputs are its own, so it has to try
/// decrypting every note it can get its hands on; cross-referencing the
/// result against GET /v1/utxos tells it which of its own outputs are still
/// spendable. Only the well-known genesis output has no note (its blinding
/// is a fixed public constant, not something anyone needs to recover via
/// decryption) - every other output, including coinbase rewards (see
/// wallet::note::coinbase_note_key), carries one and is skipped here only if
/// note decryption itself fails.
pub async fn handle_scan_outputs(
    chain: Arc<Mutex<ChainState>>,
) -> Result<impl warp::Reply, Infallible> {
    let (blocks, _) = {
        let c = chain.lock().unwrap();
        c.get_blocks_from(0, usize::MAX)
    };

    let entries: Vec<ScanOutputEntry> = blocks.iter()
        .flat_map(|block| all_outputs(block))
        .filter(|o| !o.note.is_empty())
        .map(|o| ScanOutputEntry {
            commitment_hex: commitment_hex(&o.commitment),
            note_hex: to_hex(&o.note),
        })
        .collect();

    Ok(warp::reply::json(&entries))
}

#[derive(serde::Deserialize)]
pub struct BlocksQuery {
    pub limit: Option<usize>,
}

pub async fn handle_blocks_list(
    query: BlocksQuery,
    chain: Arc<Mutex<ChainState>>,
) -> Result<impl warp::Reply, Infallible> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);

    let (blocks, prune_metas) = {
        let c = chain.lock().unwrap();
        let from_height = c.current_height.saturating_sub(limit.saturating_sub(1) as u64);
        let (blocks, _has_more) = c.get_blocks_from(from_height, limit);
        let prune_metas: Vec<Option<BlockPruneMeta>> = blocks.iter()
            .map(|b| c.prune_meta.get(&b.header.hash()).cloned())
            .collect();
        (blocks, prune_metas)
    };

    let mut summaries: Vec<BlockSummary> = blocks.iter().zip(prune_metas.iter())
        .map(|(b, m)| to_summary(b, m.as_ref()))
        .collect();
    summaries.reverse(); // newest first
    Ok(warp::reply::json(&summaries))
}

pub async fn handle_block_detail(
    height: u64,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, Infallible> {
    let block_and_meta = {
        let c = chain.lock().unwrap();
        let (blocks, _) = c.get_blocks_from(height, 1);
        blocks.into_iter().find(|b| b.header.height == height)
            .map(|b| {
                let meta = c.prune_meta.get(&b.header.hash()).cloned();
                (b, meta)
            })
    };

    match block_and_meta {
        Some((b, meta)) => Ok(Box::new(warp::reply::json(&to_detail(&b, meta.as_ref())))),
        None => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({ "error": "block not found" })),
            warp::http::StatusCode::NOT_FOUND,
        ))),
    }
}

pub async fn handle_validators(
    chain: Arc<Mutex<ChainState>>,
) -> Result<impl warp::Reply, Infallible> {
    let validators: Vec<ValidatorInfo> = {
        let c = chain.lock().unwrap();
        c.active_validators.iter().map(|v| ValidatorInfo {
            commitment: commitment_hex(&v.commitment),
            value: v.value,
        }).collect()
    };
    Ok(warp::reply::json(&validators))
}

#[derive(Serialize)]
pub struct TransactionSummary {
    pub block_height: u64,
    pub block_hash: String,
    pub excess: String,
    pub fee: u64,
}

pub async fn handle_transactions_list(
    query: BlocksQuery,
    chain: Arc<Mutex<ChainState>>,
) -> Result<impl warp::Reply, Infallible> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    // Scanning depth: pull enough recent blocks to have a good chance of
    // gathering `limit` kernels (blocks may have zero user kernels), same
    // devnet-scale scan cost the rest of the explorer already accepts.
    const SCAN_BLOCKS: usize = 100;

    let blocks = {
        let c = chain.lock().unwrap();
        let from_height = c.current_height.saturating_sub(SCAN_BLOCKS.saturating_sub(1) as u64);
        let (blocks, _has_more) = c.get_blocks_from(from_height, SCAN_BLOCKS);
        blocks
    };

    let mut summaries: Vec<TransactionSummary> = Vec::new();
    for block in blocks.iter().rev() {
        let block_hash = to_hex(&block.header.hash());
        for kernel in all_kernels(block) {
            summaries.push(TransactionSummary {
                block_height: block.header.height,
                block_hash: block_hash.clone(),
                excess: commitment_hex(&kernel.excess),
                fee: kernel.fee,
            });
            if summaries.len() >= limit {
                return Ok(warp::reply::json(&summaries));
            }
        }
    }
    Ok(warp::reply::json(&summaries))
}

#[derive(serde::Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub result_type: String,
    pub height: Option<u64>,
}

fn not_found() -> SearchResult {
    SearchResult { result_type: "not_found".to_string(), height: None }
}

pub async fn handle_search(
    query: SearchQuery,
    chain: Arc<Mutex<ChainState>>,
) -> Result<impl warp::Reply, Infallible> {
    let q = query.q.trim();

    // 1. Numeric input: treat as a block height.
    if let Ok(height) = q.parse::<u64>() {
        let found = {
            let c = chain.lock().unwrap();
            let (blocks, _) = c.get_blocks_from(height, 1);
            blocks.iter().any(|b| b.header.height == height)
        };
        if found {
            return Ok(warp::reply::json(&SearchResult { result_type: "block".to_string(), height: Some(height) }));
        }
        return Ok(warp::reply::json(&not_found()));
    }

    // 2. 64-char hex input: try block hash, then kernel excess, then a UTXO commitment.
    if q.len() == 64 && q.chars().all(|c| c.is_ascii_hexdigit()) {
        let mut bytes = [0u8; 32];
        let mut valid = true;
        for i in 0..32 {
            match u8::from_str_radix(&q[i * 2..i * 2 + 2], 16) {
                Ok(b) => bytes[i] = b,
                Err(_) => { valid = false; break; }
            }
        }

        if valid {
            let c = chain.lock().unwrap();

            if let Some(block) = c.blocks.get(&bytes) {
                return Ok(warp::reply::json(&SearchResult { result_type: "block".to_string(), height: Some(block.header.height) }));
            }

            let query_hex = q.to_lowercase();
            for block in c.blocks.values() {
                if all_kernels(block).iter().any(|k| commitment_hex(&k.excess) == query_hex) {
                    return Ok(warp::reply::json(&SearchResult { result_type: "transaction".to_string(), height: Some(block.header.height) }));
                }
            }
            for block in c.blocks.values() {
                let matches_output = all_outputs(block).iter().any(|o| commitment_hex(&o.commitment) == query_hex);
                let matches_input = all_inputs(block).iter().any(|i| commitment_hex(&i.commitment) == query_hex);
                if matches_output || matches_input {
                    return Ok(warp::reply::json(&SearchResult { result_type: "commitment".to_string(), height: Some(block.header.height) }));
                }
            }
        }
    }

    Ok(warp::reply::json(&not_found()))
}

pub async fn handle_index() -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::html(EXPLORER_HTML))
}

const EXPLORER_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Haze &mdash; Explorer</title>
<script>
  // Applied synchronously, before CSS paints, to avoid a flash of the wrong
  // theme on load - a saved choice always wins; otherwise falls back to the
  // OS preference.
  (function () {
    var saved = localStorage.getItem("hazeTheme");
    var theme = saved || (window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark");
    document.documentElement.setAttribute("data-theme", theme);
  })();
</script>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,340..600&family=Public+Sans:wght@400;500;600;700&family=IBM+Plex+Mono:wght@400;500&display=swap" rel="stylesheet">
<style>
  :root {
    color-scheme: dark;
    --fog-0: oklch(0.15 0.014 292);
    --fog-1: oklch(0.19 0.016 292);
    --fog-2: oklch(0.24 0.018 292);
    --fog-3: oklch(0.32 0.02 292);
    --mist: oklch(0.68 0.05 285);
    --ink: oklch(0.93 0.012 90);
    --ink-dim: oklch(0.93 0.012 90 / 0.62);
    --ink-faint: oklch(0.93 0.012 90 / 0.34);
    --amber: oklch(0.78 0.15 70);
    --amber-dim: oklch(0.78 0.15 70 / 0.16);
    --violet: oklch(0.72 0.09 300);
    --ok: oklch(0.75 0.14 155);
    --danger: oklch(0.7 0.16 25);
    --glow-1: oklch(0.26 0.03 296 / 0.55);
    --glow-2: oklch(0.22 0.03 70 / 0.12);
    --panel-gradient-top: oklch(0.22 0.02 292);
    --font-display: "Fraunces", serif;
    --font-body: "Public Sans", sans-serif;
    --font-mono: "IBM Plex Mono", monospace;
  }

  /* Same fog/mist language, inverted for a light surface - fog goes from
     near-black to near-white, ink goes from near-white to near-black, mist/
     amber/violet/ok/danger keep their hue but deepen slightly for contrast
     against a light background. */
  :root[data-theme="light"] {
    color-scheme: light;
    --fog-0: oklch(0.98 0.006 292);
    --fog-1: oklch(0.955 0.008 292);
    --fog-2: oklch(0.90 0.01 292);
    --fog-3: oklch(0.82 0.014 292);
    --mist: oklch(0.52 0.09 285);
    --ink: oklch(0.22 0.014 292);
    --ink-dim: oklch(0.22 0.014 292 / 0.68);
    --ink-faint: oklch(0.22 0.014 292 / 0.42);
    --amber: oklch(0.58 0.15 70);
    --amber-dim: oklch(0.58 0.15 70 / 0.12);
    --violet: oklch(0.55 0.11 300);
    --ok: oklch(0.52 0.14 155);
    --danger: oklch(0.55 0.18 25);
    --glow-1: oklch(0.9 0.02 296 / 0.5);
    --glow-2: oklch(0.92 0.03 70 / 0.35);
    --panel-gradient-top: oklch(0.93 0.012 292);
  }

  * { box-sizing: border-box; }

  body {
    background:
      radial-gradient(ellipse 900px 500px at 12% -10%, var(--glow-1), transparent),
      radial-gradient(ellipse 700px 500px at 100% 10%, var(--glow-2), transparent),
      var(--fog-0);
    color: var(--ink);
    font-family: var(--font-body);
    margin: 0;
    min-height: 100vh;
    padding: clamp(16px, 4vw, 44px) clamp(16px, 5vw, 64px) 80px;
  }

  a { color: inherit; }

  /* ---------- Top bar ---------- */
  .topbar {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 32px;
    flex-wrap: wrap;
    margin-bottom: clamp(28px, 4vw, 48px);
  }
  .brand { display: flex; align-items: baseline; gap: 12px; }
  .brand-mark {
    font-family: var(--font-display);
    font-size: 30px;
    font-style: italic;
    font-weight: 480;
    color: var(--amber);
  }
  .brand-name {
    font-family: var(--font-display);
    font-optical-sizing: auto;
    font-weight: 460;
    font-size: clamp(26px, 3vw, 34px);
    letter-spacing: -0.01em;
  }
  .brand-tag {
    font-size: 12.5px;
    color: var(--ink-faint);
    text-transform: uppercase;
    letter-spacing: 0.16em;
    align-self: center;
    padding-left: 4px;
    border-left: 1px solid var(--fog-3);
    margin-left: 2px;
  }

  .topbar-right { display: flex; align-items: center; gap: 18px; flex-wrap: wrap; }

  .node-indicator {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--ink-faint);
    background: var(--fog-1);
    border: 1px solid var(--fog-3);
    border-radius: 3px;
    padding: 7px 12px;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 7px;
    transition: border-color 0.2s ease, color 0.2s ease;
  }
  .node-indicator:hover { border-color: var(--mist); color: var(--ink-dim); }
  .node-indicator .node-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--ink-faint); flex-shrink: 0; }
  .node-indicator .node-dot.online { background: var(--ok); box-shadow: 0 0 0 3px color-mix(in oklch, var(--ok) 15%, transparent); }
  .node-indicator .node-dot.offline { background: var(--danger); box-shadow: 0 0 0 3px color-mix(in oklch, var(--danger) 15%, transparent); }

  .theme-toggle {
    font-family: var(--font-mono);
    color: var(--ink-faint);
    background: var(--fog-1);
    border: 1px solid var(--fog-3);
    border-radius: 3px;
    width: 30px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: border-color 0.2s ease, color 0.2s ease;
  }
  .theme-toggle:hover { border-color: var(--mist); color: var(--ink-dim); }
  .theme-toggle svg { width: 15px; height: 15px; }
  .theme-toggle .icon-moon { display: none; }
  :root[data-theme="light"] .theme-toggle .icon-sun { display: none; }
  :root[data-theme="light"] .theme-toggle .icon-moon { display: block; }

  .search-form {
    position: relative;
    width: min(480px, 100%);
  }
  .search-form input {
    width: 100%;
    background: var(--fog-1);
    border: 1px solid var(--fog-3);
    color: var(--ink);
    font-family: var(--font-mono);
    font-size: 13.5px;
    border-radius: 3px;
    padding: 13px 44px 13px 16px;
    outline: none;
    transition: border-color 0.2s ease, background 0.2s ease;
  }
  .search-form input::placeholder { color: var(--ink-faint); font-family: var(--font-body); }
  .search-form input:focus { border-color: var(--mist); background: var(--fog-2); }
  .search-form button {
    position: absolute;
    right: 6px; top: 6px; bottom: 6px;
    width: 34px;
    background: transparent;
    border: none;
    color: var(--ink-dim);
    cursor: pointer;
    font-size: 16px;
    border-radius: 2px;
  }
  .search-form button:hover { color: var(--amber); }

  /* ---------- Stat row (asymmetric, not a repeated card grid) ---------- */
  .stat-row {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: clamp(20px, 4vw, 56px);
    align-items: end;
    padding-bottom: clamp(20px, 3vw, 32px);
    margin-bottom: clamp(24px, 3vw, 36px);
    border-bottom: 1px solid var(--fog-2);
  }
  .stat-hero .stat-label { font-size: 12px; letter-spacing: 0.12em; text-transform: uppercase; color: var(--ink-faint); margin-bottom: 6px; }
  .stat-hero .stat-value {
    font-family: var(--font-display);
    font-size: clamp(44px, 7vw, 68px);
    line-height: 0.95;
    font-weight: 460;
    font-variant-numeric: tabular-nums;
  }
  .stat-hero .stat-sub { font-family: var(--font-mono); font-size: 12px; color: var(--ink-faint); margin-top: 8px; }

  .stat-minor { display: flex; gap: clamp(24px, 4vw, 52px); flex-wrap: wrap; }
  .stat-minor .stat-item { min-width: 110px; }
  .stat-minor .stat-label { font-size: 11.5px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--ink-faint); margin-bottom: 6px; }
  .stat-minor .stat-value { font-size: 22px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .stat-minor .stat-value.accent { color: var(--amber); }
  .stat-minor .stat-note { font-size: 11.5px; color: var(--ink-faint); margin-top: 3px; }

  /* ---------- Search result (fog-clearing reveal) ---------- */
  .search-result {
    display: grid;
    grid-template-rows: 0fr;
    opacity: 0;
    filter: blur(6px);
    transition: grid-template-rows 0.5s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.4s ease, filter 0.5s ease;
    margin-bottom: 8px;
  }
  .search-result.open {
    grid-template-rows: 1fr;
    opacity: 1;
    filter: blur(0);
    margin-bottom: clamp(20px, 3vw, 32px);
  }
  .search-result > div { overflow: hidden; }
  .search-result-inner {
    border: 1px solid var(--amber-dim);
    background: linear-gradient(180deg, var(--panel-gradient-top), var(--fog-1));
    border-radius: 4px;
    padding: 18px 22px;
  }
  .search-result-inner .sr-label { font-size: 11px; text-transform: uppercase; letter-spacing: 0.12em; color: var(--amber); margin-bottom: 8px; }
  .search-result-inner .sr-empty { color: var(--ink-dim); font-size: 14px; }
  .search-result-inner .sr-link { color: var(--mist); text-decoration: underline; text-underline-offset: 3px; cursor: pointer; }

  /* ---------- Panels ---------- */
  .panels {
    display: grid;
    grid-template-columns: 1.2fr 1fr;
    gap: clamp(20px, 3vw, 32px);
  }
  @media (max-width: 880px) {
    .panels { grid-template-columns: 1fr; }
  }

  .panel h2 {
    font-family: var(--font-display);
    font-weight: 460;
    font-size: 19px;
    margin: 0 0 14px 2px;
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .panel h2 .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--ok); display: inline-block; box-shadow: 0 0 0 3px color-mix(in oklch, var(--ok) 15%, transparent); }

  .row-list { border-top: 1px solid var(--fog-2); }
  .row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 14px;
    padding: 12px 4px;
    border-bottom: 1px solid var(--fog-2);
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .row:hover { background: var(--fog-1); }
  .row-badge {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--fog-0);
    background: var(--mist);
    padding: 4px 9px;
    border-radius: 3px;
    font-weight: 500;
    white-space: nowrap;
  }
  .row-badge.tx { background: var(--amber); }
  .row-main { min-width: 0; }
  .row-hash { font-family: var(--font-mono); font-size: 13px; color: var(--ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-sub { font-size: 11.5px; color: var(--ink-faint); margin-top: 2px; }
  .row-meta { text-align: right; font-size: 11.5px; color: var(--ink-faint); white-space: nowrap; }
  .row-meta .fee { color: var(--amber); font-family: var(--font-mono); }

  .empty-state { padding: 28px 4px; color: var(--ink-faint); font-size: 13.5px; border-bottom: 1px solid var(--fog-2); }
  .empty-state .empty-link { color: var(--mist); text-decoration: underline; text-underline-offset: 3px; cursor: pointer; }

  /* ---------- Detail expansion ---------- */
  .detail-box {
    grid-column: 1 / -1;
    background: var(--fog-1);
    border: 1px solid var(--fog-3);
    border-left: 2px solid var(--amber);
    border-radius: 3px;
    padding: 16px 18px;
    margin: 2px 0 10px 0;
    font-size: 13px;
  }
  .detail-box .d-section { margin-bottom: 12px; }
  .detail-box .d-label { color: var(--ink-faint); font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.1em; margin-bottom: 5px; }
  .detail-box .d-hash { font-family: var(--font-mono); color: var(--violet); display: block; word-break: break-all; line-height: 1.6; }
  .detail-box .d-hash.highlight { color: var(--amber); background: var(--amber-dim); border-radius: 3px; padding: 2px 6px; margin: -2px -6px; }
  .detail-box .d-empty { color: var(--ink-faint); font-style: italic; }

  ::selection { background: var(--amber-dim); }
</style>
</head>
<body>

  <header class="topbar">
    <div class="brand">
      <span class="brand-mark">&#9686;</span>
      <span class="brand-name">Haze</span>
      <span class="brand-tag">explorer</span>
    </div>
    <div class="topbar-right">
      <button class="theme-toggle" id="theme-toggle" title="Toggle light / dark mode" aria-label="Toggle light / dark mode">
        <svg class="icon-sun" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"></circle><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"></path></svg>
        <svg class="icon-moon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path></svg>
      </button>
      <div class="node-indicator" id="node-indicator" title="Click to change the Haze node this explorer talks to">
        <span class="node-dot" id="node-dot"></span>
        <span id="node-label">&mdash;</span>
      </div>
      <form class="search-form" id="search-form">
        <input id="search-input" type="text" placeholder="Block height, block hash, or transaction / commitment hash" autocomplete="off" />
        <button type="submit" aria-label="Search">&#8594;</button>
      </form>
    </div>
  </header>

  <section class="stat-row">
    <div class="stat-hero">
      <div class="stat-label">Chain Height</div>
      <div class="stat-value" id="stat-height">&mdash;</div>
      <div class="stat-sub" id="stat-tip">&mdash;</div>
    </div>
    <div class="stat-minor">
      <div class="stat-item">
        <div class="stat-label">Validators</div>
        <div class="stat-value" id="stat-validators">&mdash;</div>
        <div class="stat-note" id="stat-validators-note"></div>
      </div>
      <div class="stat-item">
        <div class="stat-label">Mempool</div>
        <div class="stat-value accent" id="stat-mempool">&mdash;</div>
        <div class="stat-note">pending transactions</div>
      </div>
      <div class="stat-item">
        <div class="stat-label">Latest Block</div>
        <div class="stat-value" id="stat-time" style="font-size: 16px;">&mdash;</div>
        <div class="stat-note">local time</div>
      </div>
    </div>
  </section>

  <div class="search-result" id="search-result">
    <div><div class="search-result-inner" id="search-result-inner"></div></div>
  </div>

  <main class="panels">
    <section class="panel">
      <h2><span class="dot"></span> Latest Blocks</h2>
      <div class="row-list" id="blocks-body"></div>
    </section>
    <section class="panel">
      <h2><span class="dot"></span> Latest Transactions</h2>
      <div class="row-list" id="txs-body"></div>
    </section>
    <section class="panel">
      <h2><span class="dot"></span> Marketplace Listings</h2>
      <div class="row-list" id="listings-body"></div>
    </section>
  </main>

<script>
// ---------- Node API base resolution ----------
// Standalone deployment (unlike the embedded explorer served by the node
// itself) has no same-origin API to call, so the node URL is user-configurable:
// ?api=<url> query param (persisted to localStorage) > previously saved value
// > a localhost default for local development.
const DEFAULT_API_BASE = "http://localhost:8332";
const STORAGE_KEY = "hazeApiBase";

document.getElementById("theme-toggle").addEventListener("click", () => {
  const current = document.documentElement.getAttribute("data-theme");
  const next = current === "light" ? "dark" : "light";
  document.documentElement.setAttribute("data-theme", next);
  localStorage.setItem("hazeTheme", next);
});

function resolveApiBase() {
  const params = new URLSearchParams(window.location.search);
  const fromQuery = params.get("api");
  if (fromQuery) {
    localStorage.setItem(STORAGE_KEY, fromQuery);
    return fromQuery.replace(/\/+$/, "");
  }
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved) return saved.replace(/\/+$/, "");
  return DEFAULT_API_BASE;
}

let API_BASE = resolveApiBase();
let nodeReachable = false;

function renderNodeIndicator() {
  document.getElementById("node-label").textContent = API_BASE;
  const dot = document.getElementById("node-dot");
  dot.className = "node-dot " + (nodeReachable ? "online" : "offline");
}

document.getElementById("node-indicator").addEventListener("click", () => {
  const next = window.prompt("Haze node API URL (e.g. http://your-node-host:8332):", API_BASE);
  if (next && next.trim()) {
    API_BASE = next.trim().replace(/\/+$/, "");
    localStorage.setItem(STORAGE_KEY, API_BASE);
    renderNodeIndicator();
    refreshAll();
  }
});

const shortHash = (h, n) => h.slice(0, n || 14) + "&hellip;";
const timeAgo = (unixSecs) => {
  if (!unixSecs) return "—";
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - unixSecs);
  if (diff < 5) return "just now";
  if (diff < 60) return diff + "s ago";
  if (diff < 3600) return Math.floor(diff / 60) + "m ago";
  return Math.floor(diff / 3600) + "h ago";
};

async function fetchJson(path) {
  const res = await fetch(API_BASE + path);
  if (!res.ok) throw new Error("HTTP " + res.status);
  return res.json();
}

function unreachableMessage() {
  return `Can&rsquo;t reach a node at <strong>${API_BASE}</strong> &mdash; click <span class="empty-link" onclick="document.getElementById('node-indicator').click()">Node</span> above to point at a running Haze node.`;
}

async function refreshStatus() {
  const s = await fetchJson("/v1/status");
  document.getElementById("stat-height").textContent = s.height;
  document.getElementById("stat-tip").textContent = "tip " + shortHash(s.tip_hash, 18);

  if (s.active_validators === 0) {
    document.getElementById("stat-validators").textContent = "1";
    document.getElementById("stat-validators-note").textContent = "genesis default proposer";
  } else {
    document.getElementById("stat-validators").textContent = s.active_validators;
    document.getElementById("stat-validators-note").textContent = "registered";
  }
  document.getElementById("stat-mempool").textContent = s.mempool_size;
}

async function refreshBlocksTime() {
  const blocks = await fetchJson("/v1/blocks?limit=1");
  if (blocks.length) {
    document.getElementById("stat-time").textContent = timeAgo(blocks[0].timestamp);
  }
}

// Tracks which block height (if any) has its detail row expanded, so the
// periodic refresh below can restore it instead of silently discarding it.
let expandedHeight = null;

function removeExpandedDetail() {
  const existing = document.querySelector('[id^="detail-"]');
  if (existing) existing.remove();
}

function renderDetailBox(b, highlightHex) {
  const box = document.createElement("div");
  box.className = "detail-box";
  box.id = "detail-" + b.height;

  const cls = (hash) => "d-hash" + (highlightHex && hash === highlightHex ? " highlight" : "");
  const list = (items) => items.length
    ? items.map(i => `<span class="${cls(i)}">${i}</span>`).join("")
    : `<span class="d-empty">none</span>`;
  const kernelList = b.kernels.length
    ? b.kernels.map(k => `<span class="${cls(k.excess)}">${k.excess} <span style="color:var(--amber)">(fee ${k.fee})</span></span>`).join("")
    : `<span class="d-empty">none</span>`;

  box.innerHTML = `
    <div class="d-section"><div class="d-label">Full Hash</div><span class="${cls(b.hash)}">${b.hash}</span></div>
    <div class="d-section"><div class="d-label">Prev Hash</div><span class="d-hash">${b.prev_hash}</span></div>
    <div class="d-section"><div class="d-label">Nonce</div>${b.nonce}</div>
    <div class="d-section"><div class="d-label">Inputs (${b.inputs.length})</div>${list(b.inputs)}</div>
    <div class="d-section"><div class="d-label">Outputs (${b.outputs.length})</div>${list(b.outputs)}</div>
    <div class="d-section"><div class="d-label">Kernels (${b.kernels.length})</div>${kernelList}</div>
  `;
  return box;
}

async function insertBlockDetail(height, rowEl, highlightHex) {
  const b = await fetchJson(`/v1/blocks/${height}`);
  const box = renderDetailBox(b, highlightHex);
  rowEl.after(box);
  return box;
}

async function toggleBlockDetail(height, rowEl) {
  if (expandedHeight === height) {
    removeExpandedDetail();
    expandedHeight = null;
    return;
  }
  removeExpandedDetail();
  expandedHeight = height;
  await insertBlockDetail(height, rowEl);
}

function blockRow(b) {
  const row = document.createElement("div");
  row.className = "row";
  row.innerHTML = `
    <span class="row-badge">#${b.height}</span>
    <span class="row-main">
      <span class="row-hash">${shortHash(b.hash, 22)}</span>
      <div class="row-sub">proposer ${shortHash(b.proposer, 12)}</div>
    </span>
    <span class="row-meta">${timeAgo(b.timestamp)}<br>${b.num_inputs}in / ${b.num_outputs}out / ${b.num_kernels}kn</span>
  `;
  row.addEventListener("click", () => toggleBlockDetail(b.height, row));
  return row;
}

async function refreshBlocks() {
  const blocks = await fetchJson("/v1/blocks?limit=20");
  const body = document.getElementById("blocks-body");
  body.innerHTML = "";
  if (blocks.length === 0) {
    body.innerHTML = `<div class="empty-state">No blocks yet &mdash; waiting for the chain to produce its first block.</div>`;
    return;
  }
  let expandedRow = null;
  blocks.forEach(b => {
    const row = blockRow(b);
    body.appendChild(row);
    if (b.height === expandedHeight) expandedRow = row;
  });
  if (expandedHeight !== null) {
    if (expandedRow) {
      await insertBlockDetail(expandedHeight, expandedRow);
    } else {
      expandedHeight = null;
    }
  }
}

function txRow(t) {
  const row = document.createElement("div");
  row.className = "row";
  row.innerHTML = `
    <span class="row-badge tx">tx</span>
    <span class="row-main">
      <span class="row-hash">${shortHash(t.excess, 22)}</span>
      <div class="row-sub">block #${t.block_height}</div>
    </span>
    <span class="row-meta">fee<br><span class="fee">${t.fee}</span></span>
  `;
  return row;
}

async function refreshTransactions() {
  const txs = await fetchJson("/v1/transactions?limit=20");
  const body = document.getElementById("txs-body");
  body.innerHTML = "";
  if (txs.length === 0) {
    body.innerHTML = `<div class="empty-state">No transactions yet &mdash; only coinbase-free blocks so far. Submit one with the wallet CLI.</div>`;
    return;
  }
  txs.forEach(t => body.appendChild(txRow(t)));
}

// Metadata is stored on-chain as raw bytes (see core::assets::AssetRecord)
// - interpreted here as UTF-8 text, and if it happens to parse as JSON with
// a title/description/image shape, rendered as a real preview instead of
// raw text (see core::assets's own doc comment: consensus only enforces a
// length cap, everything else is a UI-layer convention).
function decodeAssetMetadata(bytes) {
  try {
    const text = new TextDecoder("utf-8", { fatal: false }).decode(new Uint8Array(bytes));
    try {
      const parsed = JSON.parse(text);
      if (parsed && typeof parsed === "object") return { title: parsed.title, description: parsed.description, image: parsed.image, raw: text };
    } catch (_) { /* not JSON - fall through to plain text */ }
    return { raw: text };
  } catch (_) {
    return { raw: "" };
  }
}

function listingRow(l, asset) {
  const row = document.createElement("div");
  row.className = "row";
  const meta = asset ? decodeAssetMetadata(asset.metadata) : { raw: "" };
  const preview = meta.image
    ? `<img src="${meta.image}" alt="" style="width:36px;height:36px;object-fit:cover;border-radius:6px;vertical-align:middle;margin-right:8px;" onerror="this.style.display='none'">`
    : "";
  const title = meta.title || l.asset_id;
  const description = (meta.description || meta.raw || "").slice(0, 80);
  row.innerHTML = `
    <span class="row-badge">${l.price}</span>
    <span class="row-main">
      <span class="row-hash">${preview}${title}</span>
      <div class="row-sub">${description}</div>
    </span>
    <span class="row-meta">seller<br>${shortHash(bytesHex(l.seller_pubkey), 12)}</span>
  `;
  return row;
}

function bytesHex(byteArray) {
  return byteArray.map(b => b.toString(16).padStart(2, "0")).join("");
}

async function refreshListings() {
  const listings = await fetchJson("/v1/marketplace/listings?limit=20");
  const body = document.getElementById("listings-body");
  body.innerHTML = "";
  if (listings.length === 0) {
    body.innerHTML = `<div class="empty-state">No marketplace listings yet.</div>`;
    return;
  }
  const assets = await Promise.all(listings.map(l => fetchJson(`/v1/assets/${encodeURIComponent(l.asset_id)}`).catch(() => null)));
  listings.forEach((l, i) => body.appendChild(listingRow(l, assets[i])));
}

async function runSearch(query) {
  const panel = document.getElementById("search-result");
  const inner = document.getElementById("search-result-inner");
  if (!query.trim()) {
    panel.classList.remove("open");
    return;
  }

  try {
    const result = await fetchJson("/v1/search?q=" + encodeURIComponent(query.trim()));

    if (result.result_type === "not_found") {
      inner.innerHTML = `<div class="sr-label">No match</div><div class="sr-empty">Nothing found for "${query}". Try a block height, a full block hash, or a transaction/commitment hash (64 hex characters).</div>`;
    } else {
      const typeLabel = result.result_type === "block" ? "Block found"
        : result.result_type === "transaction" ? "Transaction found in block"
        : "Commitment found in block";
      // Jumps straight to the matching block's detail (highlighting the
      // specific kernel/commitment/hash that matched) instead of making the
      // user click a second time - deep links (?q=...) should land on the
      // actual thing being searched for, not one click short of it.
      inner.innerHTML = `<div class="sr-label">${typeLabel}</div><div class="sr-empty">Height <span class="sr-link" id="sr-jump">#${result.height}</span></div>`;
      const b = await fetchJson(`/v1/blocks/${result.height}`);
      const highlightHex = result.result_type === "block" ? b.hash : query.trim();
      inner.appendChild(renderDetailBox(b, highlightHex));
      document.getElementById("sr-jump").addEventListener("click", () => {
        document.getElementById("sr-jump").scrollIntoView({ behavior: "smooth", block: "center" });
      });
    }
  } catch (e) {
    inner.innerHTML = `<div class="sr-label">Search unavailable</div><div class="sr-empty">${unreachableMessage()}</div>`;
  }
  panel.classList.add("open");
}

document.getElementById("search-form").addEventListener("submit", (e) => {
  e.preventDefault();
  runSearch(document.getElementById("search-input").value);
});

async function refreshAll() {
  try {
    await Promise.all([refreshStatus(), refreshBlocks(), refreshTransactions(), refreshBlocksTime(), refreshListings()]);
    nodeReachable = true;
  } catch (e) {
    nodeReachable = false;
    document.getElementById("blocks-body").innerHTML = `<div class="empty-state">${unreachableMessage()}</div>`;
    document.getElementById("txs-body").innerHTML = `<div class="empty-state">${unreachableMessage()}</div>`;
    document.getElementById("listings-body").innerHTML = `<div class="empty-state">${unreachableMessage()}</div>`;
    ["stat-height", "stat-tip", "stat-validators", "stat-mempool", "stat-time"].forEach(id => {
      document.getElementById(id).textContent = "—";
    });
  }
  renderNodeIndicator();
}

renderNodeIndicator();
refreshAll();
setInterval(refreshAll, 5000);

// Deep-link support: ?q=<hex> runs the same search a user would type in by
// hand, so other pages (e.g. the wallet, after broadcasting) can link
// straight to a transaction/block/commitment instead of just saying "done".
const deepLinkQuery = new URLSearchParams(window.location.search).get("q");
if (deepLinkQuery) {
  document.getElementById("search-input").value = deepLinkQuery;
  runSearch(deepLinkQuery);
}
</script>
</body>
</html>
"#;
