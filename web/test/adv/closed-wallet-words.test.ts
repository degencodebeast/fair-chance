// Review finding, kept as a test: wallet errors that do not say the wallet closed still get "sent nothing".
import { describe, expect, it } from "vitest";
import { sendAndRead } from "../../src/trade-actions.ts";
import type { WalletSession } from "../../src/tx-types.ts";

const tx = [{ receiverId: "moth.factory.testnet", actions: [] }] as never;
const failing = (e: unknown): WalletSession => ({ accountId: "you.testnet", send: async () => { throw e; } }) as unknown as WalletSession;

describe("wallet errors with an unknown outcome", () => {
  for (const msg of [
    "RPC connection closed before the final outcome",
    "Promise rejected: timeout waiting for the transaction status",
    "Request cancelled after the transaction was broadcast",
    "Access denied by the RPC node while waiting for the final outcome",
  ]) {
    it(msg, async () => {
      const out = await sendAndRead(failing(new Error(msg)), tx);
      expect(out.kind).toBe("unknown");
    });
  }
});
