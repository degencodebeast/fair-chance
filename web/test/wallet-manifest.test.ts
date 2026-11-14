import { describe, expect, it } from "vitest";
import { connectorOptions } from "../src/wallet-adapter.ts";
import { WALLET_MANIFEST } from "../src/wallet-manifest.ts";
import { WALLET_NAME } from "../src/try-it-copy.ts";

describe("the pinned wallet manifest", () => {
  it("is passed to the wallet library as an object, so nothing loads a wallet list at run time", () => {
    const manifest = connectorOptions().manifest;
    expect(typeof manifest).toBe("object");
    expect(manifest).toBe(WALLET_MANIFEST);
  });

  it("holds one wallet: Meteor Wallet, which works on testnet and sends a list of transactions", () => {
    expect(WALLET_MANIFEST.wallets.map((w) => w.id)).toEqual(["meteor-wallet"]);
    for (const w of WALLET_MANIFEST.wallets) {
      expect(w.type).toBe("sandbox");
      expect(w.features.testnet).toBe(true);
      expect(w.features.signAndSendTransactions).toBe(true);
      expect(w.permissions.walletConnect).toBeFalsy();
    }
  });

  it("loads the wallet code from one fixed commit, not from a branch that can change", () => {
    for (const w of WALLET_MANIFEST.wallets) {
      expect(w.executor).toMatch(/^https:\/\/raw\.githubusercontent\.com\/Meteor-Wallet\/meteor_wallet_sdk\/[0-9a-f]{40}\/storage\/meteor-near-connect-latest\.js$/);
    }
  });

  it("draws the wallet picture from the page itself, so the picker fetches no picture", () => {
    for (const w of WALLET_MANIFEST.wallets) expect(w.icon.startsWith("data:image/svg+xml,")).toBe(true);
  });

  it("names the same wallet as the how to try it box", () => {
    expect(WALLET_NAME).toContain("Meteor Wallet");
  });
});
