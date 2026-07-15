/** Thin HTTP client for a Haze node's public REST API - no crypto here,
 * just the wire format. Mirrors haze-wallet-web/index.html's fetch calls
 * exactly, since that's the tested, working reference implementation of
 * this protocol. */

export interface NameRecord {
  owner_pubkey: number[];
  resolves_to: number[];
}

export interface InboxMessage {
  from_pubkey_hex: string;
  kind: string;
  payload_json: string;
}

export class NodeApi {
  constructor(private baseUrl: string) {}

  private url(path: string): string {
    return this.baseUrl.replace(/\/+$/, "") + path;
  }

  async status(): Promise<{ height: number; tip_hash: string; active_validators: number; mempool_size: number }> {
    const res = await fetch(this.url("/v1/status"));
    if (!res.ok) throw new Error(`node status check failed: HTTP ${res.status}`);
    return (await res.json()) as { height: number; tip_hash: string; active_validators: number; mempool_size: number };
  }

  async utxos(): Promise<number[][]> {
    const res = await fetch(this.url("/v1/utxos"));
    if (!res.ok) throw new Error(`failed to fetch utxos: HTTP ${res.status}`);
    return (await res.json()) as number[][];
  }

  async feeEstimate(): Promise<{ suggested_fee: number; suggested_name_fee: number }> {
    const res = await fetch(this.url("/v1/fee-estimate"));
    if (!res.ok) throw new Error(`failed to fetch fee estimate: HTTP ${res.status}`);
    return (await res.json()) as { suggested_fee: number; suggested_name_fee: number };
  }

  async resolveName(name: string): Promise<NameRecord | null> {
    const res = await fetch(this.url("/v1/names/" + encodeURIComponent(name)));
    if (res.status === 404) return null;
    if (!res.ok) throw new Error(`name lookup failed: HTTP ${res.status}`);
    return (await res.json()) as NameRecord;
  }

  async submitTransaction(transactionJson: string): Promise<void> {
    const res = await fetch(this.url("/v1/transactions"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: transactionJson,
    });
    if (!res.ok) {
      const text = await res.text().catch(() => "");
      throw new Error(`node rejected transaction: ${text || res.status}`);
    }
  }

  async registerNameSponsored(requestJson: string): Promise<void> {
    const res = await fetch(this.url("/v1/names/register-sponsored"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestJson,
    });
    if (!res.ok) {
      const errJson = (await res.json().catch(() => ({}))) as { error?: string };
      throw new Error(errJson.error || `HTTP ${res.status}`);
    }
  }

  /** Every inbox write is signed by the sender (see api::inbox's
   * inbox_post_signing_message) - the caller supplies signatureHex, this
   * class just carries it over the wire. */
  async postInbox(toPubkeyHex: string, fromPubkeyHex: string, kind: string, payloadJson: string, signatureHex: string): Promise<void> {
    const res = await fetch(this.url("/v1/inbox/" + toPubkeyHex), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ from_pubkey_hex: fromPubkeyHex, kind, payload_json: payloadJson, signature_hex: signatureHex }),
    });
    if (!res.ok) throw new Error(`failed to deliver inbox message: HTTP ${res.status}`);
  }

  /** Draining an inbox requires proving ownership of the pubkey being
   * polled (inbox_poll_signing_message) - timestamp/signatureHex are the
   * caller's proof, computed fresh per call since they bind to the
   * current time. */
  async getInbox(pubkeyHex: string, timestamp: number, signatureHex: string): Promise<InboxMessage[]> {
    const res = await fetch(this.url(`/v1/inbox/${pubkeyHex}?timestamp=${timestamp}&signature_hex=${signatureHex}`));
    if (!res.ok) return [];
    return (await res.json()) as InboxMessage[];
  }
}
