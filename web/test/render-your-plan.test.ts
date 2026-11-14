// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import type { CoinReads } from "../src/coin-page.ts";
import { drawYourPlan } from "../src/render-your-plan.ts";
import { walletControl, type WalletAccount, type WalletControl } from "../src/wallet-session.ts";
import { copyViolations } from "./copy-rules.ts";

const PAGE = "https://example.test/?network=testnet&coin=moth.factory.testnet";
const reads = { metadata: { kind: "unavailable" }, totalSupply: { kind: "unavailable" } } as unknown as CoinReads;

function wallet(): { control: WalletControl; signIn: (id: string) => void; signOut: () => void } {
  let onIn: (a: WalletAccount) => void = () => {};
  let onOut: () => void = () => {};
  const control = walletControl(PAGE, () => ({
    connect: async () => ({ accountId: "you.testnet", network: "testnet" }),
    restore: async () => null,
    send: async () => [],
    signOut: async () => {},
    onSignIn: (l) => void (onIn = l),
    onSignOut: (l) => void (onOut = l),
  }));
  return { control, signIn: (accountId) => onIn({ accountId, network: "testnet" }), signOut: () => onOut() };
}

const loaded = { account: { kind: "unavailable" as const }, asked: null, quote: null, earlySupplyQuote: null };

async function draw(w: WalletControl, load = vi.fn(async () => loaded)) {
  const root = drawYourPlan(reads, w, load);
  document.body.replaceChildren(root);
  await w.connect().catch(() => {});
  return { root, load };
}

describe("the sell limit card", () => {
  it("is named Your sell limit, with the new intro and no placeholder number", () => {
    const { control } = wallet();
    const root = drawYourPlan(reads, control, vi.fn(async () => loaded));
    expect(root.querySelector("h2")?.textContent).toBe("Your sell limit");
    expect(root.textContent).toContain("How much this account can sell now, and when more opens up.");
    expect(root.textContent).not.toContain("The coin works these out");
    expect(root.querySelector<HTMLInputElement>("#plan-amount")!.placeholder).toBe("Amount");
    expect(copyViolations(root.textContent ?? "")).toEqual([]);
  });

  it("fills in the connected account and shows its limit at once", async () => {
    const { control } = wallet();
    const { root, load } = await draw(control);
    expect(root.querySelector<HTMLInputElement>("#plan-account")!.value).toBe("you.testnet");
    await vi.waitFor(() => expect(root.querySelector("#plan-result")?.textContent).toContain("Your sell limit"));
    expect(load).toHaveBeenCalledWith({ accountId: "you.testnet", asked: null });
  });

  it("leaves the field empty with no wallet", () => {
    const { control } = wallet();
    const root = drawYourPlan(reads, control, vi.fn(async () => loaded));
    expect(root.querySelector<HTMLInputElement>("#plan-account")!.value).toBe("");
  });

  it("keeps what the visitor typed when the wallet signs in or out", async () => {
    const { control } = wallet();
    const root = drawYourPlan(reads, control, vi.fn(async () => loaded));
    document.body.replaceChildren(root);
    const input = root.querySelector<HTMLInputElement>("#plan-account")!;
    input.value = "other.testnet";
    input.dispatchEvent(new Event("input"));
    await control.connect();
    expect(input.value).toBe("other.testnet");
  });
});
