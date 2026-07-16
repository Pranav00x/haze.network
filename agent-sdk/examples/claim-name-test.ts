// Verifies claimName()'s error path (no HAZE_TREASURY_BLINDING configured
// on this node, the realistic state on both this local devnet and the live
// haze-b3l9.onrender.com node right now) - the happy path needs the real
// treasury secret, deliberately not used here, see the session note.
import { HazeAgent } from "../src/index.js";

const NODE_URL = process.env.HAZE_NODE_URL ?? "http://localhost:8332";

async function main() {
  const { agent } = await HazeAgent.create({ nodeUrl: NODE_URL });
  console.log("Agent pubkey:", agent.pubkeyHex);

  try {
    await agent.claimName("test-agent-" + Date.now());
    console.log("UNEXPECTED: claimName succeeded (treasury must be configured on this node)");
    process.exit(1);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.log("claimName failed as expected:", message);
    const looksRight = /depleted|treasury|not configured|sponsor/i.test(message);
    console.log(looksRight ? "PASS - error is a real, informative sponsor-unavailable message, not a crash" : "FAIL - unexpected error shape");
    process.exit(looksRight ? 0 : 1);
  }
}

main().catch((err) => {
  console.error("Test script crashed:", err);
  process.exit(1);
});
