// Real end-to-end demo: two independent agents, one pays the other, both
// running their own poll loop exactly as a real deployment would. Talks to
// a real local devnet node - this is not a mock.
import { HazeAgent } from "../src/index.js";
import * as wasm from "../wasm/haze_core.js";

const NODE_URL = process.env.HAZE_NODE_URL ?? "http://localhost:8332";

async function main() {
  const { agent: alice, mnemonic: aliceMnemonic } = await HazeAgent.create({ nodeUrl: NODE_URL, pollIntervalMs: 500 });
  const { agent: bob } = await HazeAgent.create({ nodeUrl: NODE_URL, pollIntervalMs: 500 });

  console.log("Alice's new mnemonic (would normally be persisted, not printed):", aliceMnemonic);
  console.log("Alice pubkey:", alice.pubkeyHex);
  console.log("Bob pubkey:  ", bob.pubkeyHex);

  // Fund Alice via the well-known devnet genesis claim (blinding=42) -
  // the same convenience the CLI's --claim-genesis and the web wallet's
  // claim_genesis() use, so this example needs no faucet/treasury setup.
  const alicePersisted = alice.export();
  const funded = wasm.claim_genesis(alicePersisted.storeBytes);
  const fundedAlice = HazeAgent.fromExport({ keystoreBytes: alicePersisted.keystoreBytes, storeBytes: funded }, { nodeUrl: NODE_URL, pollIntervalMs: 500 });

  const startBalance = await fundedAlice.refreshBalance();
  console.log("Alice starting balance:", startBalance.confirmed);

  bob.on("payment", (p) => {
    console.log(`Bob received ${p.value} from ${p.fromPubkeyHex.slice(0, 16)}…`);
  });

  fundedAlice.listen();
  bob.listen();

  console.log("Alice paying Bob 1000...");
  const txHash = await fundedAlice.pay(bob.pubkeyHex, 1000n, { timeoutMs: 60_000 });
  console.log("Payment confirmed, kernel excess:", txHash);

  // Give the node a moment to mine the block before the final balance check.
  await new Promise((r) => setTimeout(r, 3000));

  const aliceEnd = await fundedAlice.refreshBalance();
  const bobEnd = await bob.refreshBalance();
  console.log("Alice ending balance:", aliceEnd.confirmed, "(pending:", aliceEnd.pending, ")");
  console.log("Bob ending balance:  ", bobEnd.confirmed, "(pending:", bobEnd.pending, ")");

  fundedAlice.stopListening();
  bob.stopListening();
  process.exit(0);
}

main().catch((err) => {
  console.error("Example failed:", err);
  process.exit(1);
});
