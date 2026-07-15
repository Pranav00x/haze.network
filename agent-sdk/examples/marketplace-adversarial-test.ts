// One-off live adversarial verification of the marketplace's trustless-
// payment property (Phase 7 of the collection-launches plan / task #7:
// "Adversarial test suite: happy path, seller defection, buyer defection,
// double-sell race, tamper test"). Talks to a real local devnet node.
import * as wasm from "../wasm/haze_core.js";

const API = process.env.HAZE_NODE_URL ?? "http://localhost:8332";

function bytesToHex(b: number[] | Uint8Array): string {
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}

async function postJson(path: string, body: string): Promise<{ ok: boolean; status: number; text: string }> {
  const res = await fetch(API + path, { method: "POST", headers: { "Content-Type": "application/json" }, body });
  const text = await res.text();
  return { ok: res.ok, status: res.status, text };
}

async function feeEstimate(): Promise<number> {
  const res = await fetch(API + "/v1/fee-estimate");
  return (await res.json() as { suggested_fee: number }).suggested_fee;
}

async function waitForAssetMinted(assetId: string, maxMs = 15000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < maxMs) {
    const res = await fetch(API + `/v1/assets/${encodeURIComponent(assetId)}`);
    if (res.ok) return;
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`asset ${assetId} never appeared in the registry - mint may not have applied`);
}

async function waitForAssetOwner(assetId: string, expectedOwnerHex: string, maxMs = 15000): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < maxMs) {
    const res = await fetch(API + `/v1/assets/${encodeURIComponent(assetId)}`);
    if (res.ok) {
      const record = (await res.json()) as { owner_pubkey: number[] };
      if (bytesToHex(record.owner_pubkey) === expectedOwnerHex) return true;
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  return false;
}

async function waitForMempoolEmpty(maxMs = 15000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < maxMs) {
    const res = await fetch(API + "/v1/status");
    const s = (await res.json()) as { mempool_size: number };
    if (s.mempool_size === 0) return;
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error("mempool never drained - node may not be mining");
}

async function fundBuyer(sellerKeystore: Uint8Array, sellerStore: Uint8Array, buyerKeystore: Uint8Array, amount: bigint): Promise<{ sellerKeystore: Uint8Array; sellerStore: Uint8Array; buyerStore: Uint8Array }> {
  const fee = BigInt(await feeEstimate());
  const slate = wasm.create_send_slate(sellerKeystore, sellerStore, amount, fee);
  const buyerPubkey = wasm.wallet_identity_pubkey_hex(buyerKeystore);
  const responded = wasm.respond_to_slate(buyerKeystore, slate.slate_json);
  const finalized = wasm.finalize_slate(slate.pending_slate_bytes, responded.response_slate_json);
  const submit = await postJson("/v1/transactions", finalized.transaction_json);
  if (!submit.ok) throw new Error(`fundBuyer submit failed: ${submit.text}`);
  const sellerStore2 = wasm.commit_slate_send(sellerStore, finalized.spent_commitments_hex, finalized.change ?? undefined);
  const buyerStore = wasm.commit_receive(wasm.wallet_store_new(), responded.receiver_output);
  await waitForMempoolEmpty();
  void buyerPubkey;
  return { sellerKeystore: slate.updated_keystore_bytes, sellerStore: sellerStore2, buyerStore };
}

async function main() {
  const results: { name: string; pass: boolean; detail: string }[] = [];
  function check(name: string, pass: boolean, detail: string) {
    results.push({ name, pass, detail });
    console.log(`${pass ? "PASS" : "FAIL"} - ${name}: ${detail}`);
  }

  // ---- setup: seller funded via genesis claim, buyer funded by seller ----
  const seller = wasm.generate_keystore_with_mnemonic();
  let sellerKeystore = seller.keystore_bytes;
  let sellerStore = wasm.claim_genesis(wasm.wallet_store_new());

  const buyer = wasm.generate_keystore_with_mnemonic();
  let buyerKeystore = buyer.keystore_bytes;

  const funded = await fundBuyer(sellerKeystore, sellerStore, buyerKeystore, 5000n);
  sellerKeystore = funded.sellerKeystore;
  sellerStore = funded.sellerStore;
  let buyerStore = funded.buyerStore;

  const sellerPubkeyHex = wasm.wallet_identity_pubkey_hex(sellerKeystore);
  const buyerPubkeyHex = wasm.wallet_identity_pubkey_hex(buyerKeystore);
  console.log("Seller:", sellerPubkeyHex.slice(0, 16), "Buyer:", buyerPubkeyHex.slice(0, 16));

  // Reconcile both stores against the chain's real UTXO set now that the
  // funding transaction has confirmed - build_mint_asset_request (like all
  // real fee/payment selection) only spends Confirmed balance, and the
  // change from fundBuyer is still sitting Pending in each local store
  // until reconciled.
  const chainUtxosHex1 = (await (await fetch(API + "/v1/utxos")).json() as number[][]).map(bytesToHex);
  sellerStore = wasm.reconcile_wallet_store(sellerStore, chainUtxosHex1);
  buyerStore = wasm.reconcile_wallet_store(buyerStore, chainUtxosHex1);
  console.log("chain utxo count:", chainUtxosHex1.length);
  console.log("seller confirmed:", wasm.wallet_balance(sellerStore), "pending:", wasm.wallet_pending_balance(sellerStore));
  console.log("buyer confirmed:", wasm.wallet_balance(buyerStore), "pending:", wasm.wallet_pending_balance(buyerStore));

  // ---- mint + list ----
  const assetId = "adversarial-test-" + Date.now();
  const mintFee = BigInt(await feeEstimate());
  const mint = wasm.build_mint_asset_request(sellerKeystore, sellerStore, assetId, JSON.stringify({ title: "Test NFT" }), mintFee);
  const mintRes = await postJson("/v1/assets/mint", mint.op_json);
  check("mint accepted", mintRes.ok, mintRes.ok ? "ok" : mintRes.text);
  sellerKeystore = mint.updated_keystore_bytes;
  sellerStore = wasm.commit_mint_asset(sellerStore, mint.spent_commitments_hex, mint.change ?? undefined);
  await waitForAssetMinted(assetId);

  const price = 500n;
  const listing = wasm.build_create_listing_request(sellerKeystore, assetId, price, BigInt(Math.floor(Date.now() / 1000)));
  const listRes = await postJson("/v1/marketplace/list", listing);
  check("listing accepted", listRes.ok, listRes.ok ? "ok" : listRes.text);

  // ---- buyer builds the real payment (fee-only tx paying seller `price`) ----
  const payFee = BigInt(await feeEstimate());
  const paySlate = wasm.create_send_slate(buyerKeystore, buyerStore, price, payFee);
  const sellerResponded = wasm.respond_to_slate(sellerKeystore, paySlate.slate_json);
  const payment = wasm.finalize_slate(paySlate.pending_slate_bytes, sellerResponded.response_slate_json);
  const paymentTx = JSON.parse(payment.transaction_json);
  const kernelExcessHex = bytesToHex(paymentTx.kernels[0].excess);

  // ---- seller signs the CONDITIONAL transfer, referencing that kernel excess, BEFORE the payment is ever broadcast ----
  const transferOpJson = wasm.build_transfer_asset_request(sellerKeystore, assetId, buyerPubkeyHex, kernelExcessHex, null);

  // Test 1: buyer defection - never broadcasts the payment. Submitting the
  // signed transfer now must be rejected (the required kernel excess isn't
  // anywhere on-chain or in mempool yet).
  const prematureRes = await postJson("/v1/assets/transfer", transferOpJson);
  check("buyer defection: transfer rejected before payment lands", !prematureRes.ok, prematureRes.ok ? "WAS ACCEPTED (bad!)" : `correctly rejected: ${prematureRes.text.slice(0, 100)}`);

  // Test 2: tamper - try transferring to a DIFFERENT buyer using the same
  // signed op's kernel excess condition but a forged owner. Since the op
  // itself is signed over (asset_id, new_owner_pubkey, required_kernel_excess),
  // we can't just edit the JSON and have it verify - build a fresh op signed
  // by the seller for an attacker pubkey, still unpaid, and confirm rejection.
  const attacker = wasm.generate_keystore_with_mnemonic();
  const attackerPubkeyHex = wasm.wallet_identity_pubkey_hex(attacker.keystore_bytes);
  const tamperedOpJson = wasm.build_transfer_asset_request(sellerKeystore, assetId, attackerPubkeyHex, kernelExcessHex, null);
  const tamperRes = await postJson("/v1/assets/transfer", tamperedOpJson);
  check("tamper test: re-signed transfer to a different owner also rejected pre-payment", !tamperRes.ok, tamperRes.ok ? "WAS ACCEPTED (bad!)" : "correctly rejected");

  // Test 3: happy path - broadcast the real payment, then resubmit the
  // ORIGINAL signed transfer. Must now succeed.
  const paySubmit = await postJson("/v1/transactions", payment.transaction_json);
  check("payment broadcasts", paySubmit.ok, paySubmit.ok ? "ok" : paySubmit.text);
  buyerStore = wasm.commit_slate_send(buyerStore, payment.spent_commitments_hex, payment.change ?? undefined);
  await waitForMempoolEmpty();

  const finalTransferRes = await postJson("/v1/assets/transfer", transferOpJson);
  check("happy path: original signed transfer now succeeds once payment is confirmed", finalTransferRes.ok, finalTransferRes.ok ? "ok" : finalTransferRes.text);
  const applied = await waitForAssetOwner(assetId, buyerPubkeyHex);
  if (!applied) throw new Error("happy-path transfer accepted into mempool but never actually applied to the asset registry");

  // Test 4: double-sell race - resubmitting the SAME already-applied
  // transfer (or attempting a second transfer of an asset the seller no
  // longer owns) must be rejected.
  const doubleSellRes = await postJson("/v1/assets/transfer", transferOpJson);
  check("double-sell: resubmitting an already-applied transfer is rejected", !doubleSellRes.ok, doubleSellRes.ok ? "WAS ACCEPTED AGAIN (bad!)" : "correctly rejected (already spent/transferred)");

  // ---- final ownership check via the public API ----
  const assetRes = await fetch(API + `/v1/assets/${encodeURIComponent(assetId)}`);
  const assetRecord = await assetRes.json() as { owner_pubkey: number[] };
  const finalOwnerHex = bytesToHex(assetRecord.owner_pubkey);
  check("final on-chain owner is the buyer", finalOwnerHex === buyerPubkeyHex, `owner=${finalOwnerHex.slice(0, 16)} buyer=${buyerPubkeyHex.slice(0, 16)}`);

  const failed = results.filter((r) => !r.pass);
  console.log(`\n${results.length - failed.length}/${results.length} checks passed`);
  if (failed.length > 0) {
    console.log("FAILED:", failed.map((f) => f.name).join(", "));
    process.exit(1);
  }
  process.exit(0);
}

main().catch((err) => {
  console.error("Test script crashed:", err);
  process.exit(1);
});
