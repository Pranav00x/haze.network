use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use serde::Serialize;

use crate::core::chain::ChainState;
use crate::core::mempool::Mempool;
use crate::core::block::Block;

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
    pub num_inputs: usize,
    pub num_outputs: usize,
    pub num_kernels: usize,
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
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub kernels: Vec<KernelInfo>,
}

#[derive(Serialize)]
pub struct ValidatorInfo {
    pub commitment: String,
    pub value: u64,
}

fn to_summary(block: &Block) -> BlockSummary {
    BlockSummary {
        height: block.header.height,
        hash: to_hex(&block.header.hash()),
        prev_hash: to_hex(&block.header.prev_hash),
        timestamp: block.header.timestamp,
        proposer: commitment_hex(&block.header.validator_commitment),
        num_inputs: block.body.inputs.len(),
        num_outputs: block.body.outputs.len(),
        num_kernels: block.body.kernels.len(),
    }
}

fn to_detail(block: &Block) -> BlockDetail {
    BlockDetail {
        height: block.header.height,
        hash: to_hex(&block.header.hash()),
        prev_hash: to_hex(&block.header.prev_hash),
        timestamp: block.header.timestamp,
        nonce: block.header.nonce,
        proposer: commitment_hex(&block.header.validator_commitment),
        inputs: block.body.inputs.iter().map(|i| commitment_hex(&i.commitment)).collect(),
        outputs: block.body.outputs.iter().map(|o| commitment_hex(&o.commitment)).collect(),
        kernels: block.body.kernels.iter().map(|k| KernelInfo {
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

#[derive(serde::Deserialize)]
pub struct BlocksQuery {
    pub limit: Option<usize>,
}

pub async fn handle_blocks_list(
    query: BlocksQuery,
    chain: Arc<Mutex<ChainState>>,
) -> Result<impl warp::Reply, Infallible> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);

    let (blocks, current_height) = {
        let c = chain.lock().unwrap();
        let from_height = c.current_height.saturating_sub(limit.saturating_sub(1) as u64);
        let (blocks, _has_more) = c.get_blocks_from(from_height, limit);
        (blocks, c.current_height)
    };
    let _ = current_height;

    let mut summaries: Vec<BlockSummary> = blocks.iter().map(to_summary).collect();
    summaries.reverse(); // newest first
    Ok(warp::reply::json(&summaries))
}

pub async fn handle_block_detail(
    height: u64,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, Infallible> {
    let block = {
        let c = chain.lock().unwrap();
        let (blocks, _) = c.get_blocks_from(height, 1);
        blocks.into_iter().find(|b| b.header.height == height)
    };

    match block {
        Some(b) => Ok(Box::new(warp::reply::json(&to_detail(&b)))),
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

pub async fn handle_index() -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::html(EXPLORER_HTML))
}

const EXPLORER_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Haze Explorer</title>
<style>
  :root { color-scheme: dark; }
  body {
    background: #0d1117;
    color: #c9d1d9;
    font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
    margin: 0;
    padding: 24px;
  }
  h1 { color: #58a6ff; margin: 0 0 4px 0; font-size: 22px; }
  .subtitle { color: #8b949e; margin-bottom: 20px; font-size: 13px; }
  .status-bar {
    display: flex;
    gap: 24px;
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 14px 18px;
    margin-bottom: 24px;
    flex-wrap: wrap;
  }
  .status-item .label { color: #8b949e; font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; }
  .status-item .value { color: #e6edf3; font-size: 16px; margin-top: 2px; }
  h2 { color: #e6edf3; font-size: 15px; border-bottom: 1px solid #30363d; padding-bottom: 8px; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; margin-bottom: 28px; }
  th { text-align: left; color: #8b949e; font-weight: normal; padding: 6px 10px; border-bottom: 1px solid #30363d; }
  td { padding: 6px 10px; border-bottom: 1px solid #21262d; }
  tr.block-row { cursor: pointer; }
  tr.block-row:hover { background: #161b22; }
  .hash { color: #79c0ff; }
  .detail-row td { background: #0d1117; }
  .detail-box { background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 12px; }
  .detail-box .section { margin-bottom: 10px; }
  .detail-box .section-label { color: #8b949e; font-size: 11px; text-transform: uppercase; margin-bottom: 4px; }
  .detail-box .commitment { color: #7ee787; word-break: break-all; display: block; }
  .empty { color: #8b949e; font-style: italic; }
</style>
</head>
<body>
  <h1>Haze Explorer</h1>
  <div class="subtitle">Mimblewimble hides transaction amounts &mdash; inputs/outputs are shown as commitment hashes, not values.</div>

  <div class="status-bar" id="status-bar"></div>

  <h2>Recent Blocks</h2>
  <table>
    <thead>
      <tr><th>Height</th><th>Hash</th><th>Timestamp</th><th>Proposer</th><th>In</th><th>Out</th><th>Kernels</th></tr>
    </thead>
    <tbody id="blocks-body"></tbody>
  </table>

  <h2>Active Validators</h2>
  <table>
    <thead><tr><th>Commitment</th><th>Stake</th></tr></thead>
    <tbody id="validators-body"></tbody>
  </table>

<script>
const shortHash = (h) => h.slice(0, 16) + "...";

async function fetchJson(url) {
  const res = await fetch(url);
  return res.json();
}

async function refreshStatus() {
  const s = await fetchJson("/v1/status");
  document.getElementById("status-bar").innerHTML = `
    <div class="status-item"><div class="label">Height</div><div class="value">${s.height}</div></div>
    <div class="status-item"><div class="label">Tip</div><div class="value hash">${shortHash(s.tip_hash)}</div></div>
    <div class="status-item"><div class="label">Validators</div><div class="value">${s.active_validators}</div></div>
    <div class="status-item"><div class="label">Mempool</div><div class="value">${s.mempool_size}</div></div>
  `;
}

async function refreshValidators() {
  const validators = await fetchJson("/v1/validators");
  const body = document.getElementById("validators-body");
  if (validators.length === 0) {
    body.innerHTML = `<tr><td colspan="2" class="empty">No registered validators yet (genesis default proposer active)</td></tr>`;
    return;
  }
  body.innerHTML = validators.map(v => `
    <tr><td class="hash">${v.commitment}</td><td>${v.value}</td></tr>
  `).join("");
}

// Tracks which block height (if any) has its detail row expanded, so the periodic
// refresh below can restore it instead of silently discarding it.
let expandedHeight = null;

function removeExpandedDetail() {
  const existing = document.querySelector('[id^="detail-"]');
  if (existing) existing.remove();
}

async function insertBlockDetail(height, rowEl) {
  const b = await fetchJson(`/v1/blocks/${height}`);
  const detailRow = document.createElement("tr");
  detailRow.className = "detail-row";
  detailRow.id = `detail-${height}`;
  const inputsHtml = b.inputs.length
    ? b.inputs.map(i => `<span class="commitment">${i}</span>`).join("")
    : `<span class="empty">none</span>`;
  const outputsHtml = b.outputs.length
    ? b.outputs.map(o => `<span class="commitment">${o}</span>`).join("")
    : `<span class="empty">none</span>`;
  const kernelsHtml = b.kernels.length
    ? b.kernels.map(k => `<span class="commitment">${k.excess} (fee: ${k.fee})</span>`).join("")
    : `<span class="empty">none</span>`;

  detailRow.innerHTML = `<td colspan="7">
    <div class="detail-box">
      <div class="section"><div class="section-label">Full Hash</div><span class="commitment">${b.hash}</span></div>
      <div class="section"><div class="section-label">Prev Hash</div><span class="commitment">${b.prev_hash}</span></div>
      <div class="section"><div class="section-label">Nonce</div>${b.nonce}</div>
      <div class="section"><div class="section-label">Inputs (${b.inputs.length})</div>${inputsHtml}</div>
      <div class="section"><div class="section-label">Outputs (${b.outputs.length})</div>${outputsHtml}</div>
      <div class="section"><div class="section-label">Kernels (${b.kernels.length})</div>${kernelsHtml}</div>
    </div>
  </td>`;
  rowEl.after(detailRow);
}

async function showBlockDetail(height, rowEl) {
  if (expandedHeight === height) {
    removeExpandedDetail();
    expandedHeight = null;
    return;
  }
  removeExpandedDetail();
  expandedHeight = height;
  await insertBlockDetail(height, rowEl);
}

async function refreshBlocks() {
  const blocks = await fetchJson("/v1/blocks?limit=20");
  const body = document.getElementById("blocks-body");
  body.innerHTML = "";
  let expandedRowEl = null;
  blocks.forEach(b => {
    const row = document.createElement("tr");
    row.className = "block-row";
    row.innerHTML = `
      <td>${b.height}</td>
      <td class="hash">${shortHash(b.hash)}</td>
      <td>${new Date(b.timestamp * 1000).toLocaleString()}</td>
      <td class="hash">${shortHash(b.proposer)}</td>
      <td>${b.num_inputs}</td>
      <td>${b.num_outputs}</td>
      <td>${b.num_kernels}</td>
    `;
    row.addEventListener("click", () => showBlockDetail(b.height, row));
    body.appendChild(row);
    if (b.height === expandedHeight) expandedRowEl = row;
  });
  // Restore the previously expanded detail row, if its block is still in view.
  if (expandedHeight !== null) {
    if (expandedRowEl) {
      await insertBlockDetail(expandedHeight, expandedRowEl);
    } else {
      expandedHeight = null;
    }
  }
}

async function refreshAll() {
  await Promise.all([refreshStatus(), refreshBlocks(), refreshValidators()]);
}

refreshAll();
setInterval(refreshAll, 5000);
</script>
</body>
</html>
"#;
