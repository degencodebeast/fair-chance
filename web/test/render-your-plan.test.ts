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

  const bagReads = { metadata: { kind: "value", value: { symbol: "BAG", decimals: 18 } }, totalSupply: { kind: "unavailable" } } as unknown as CoinReads;
  const ONE = 10n ** 18n;
  const na = { kind: "unavailable" as const };
  function account(balance: bigint, canSell: bigint) {
    return {
      class: "small" as const,
      balance,
      markedBalance: 0n,
      unmarkedBalance: balance,
      isLarge: false,
      canSend: true,
      inCreatorGroup: false,
      maxReceive: na,
      unmarked: na,
      marked: na,
      maxFullFillNow: { kind: "value" as const, value: canSell },
      penaltyAppliesAbove: na,
    };
  }
  async function drawWith(acc: ReturnType<typeof account>, signedIn = true) {
    const { control } = wallet();
    const load = vi.fn(async () => ({ ...loaded, account: { kind: "value" as const, value: acc } }));
    const root = drawYourPlan(bagReads, control, load as never);
    document.body.replaceChildren(root);
    if (signedIn) await control.connect();
    return root;
  }

  it("labels the button Check with no amount and Preview sell with an amount", () => {
    const { control } = wallet();
    const root = drawYourPlan(reads, control, vi.fn(async () => loaded));
    const button = root.querySelector<HTMLButtonElement>("button[type=submit]")!;
    const amount = root.querySelector<HTMLInputElement>("#plan-amount")!;
    expect(button.textContent).toBe("Check");
    amount.value = "100";
    amount.dispatchEvent(new Event("input"));
    expect(button.textContent).toBe("Preview sell");
    amount.value = "  ";
    amount.dispatchEvent(new Event("input"));
    expect(button.textContent).toBe("Check");
  });

  it("says you hold none for a zero balance, not that no limit applies", async () => {
    const root = await drawWith(account(0n, 0n));
    await vi.waitFor(() => expect(root.querySelector("#plan-card")).not.toBeNull());
    const text = root.querySelector("#plan-result")!.textContent ?? "";
    expect(text).toContain("You hold no BAG yet.");
    expect(text).not.toContain("No limit applies");
  });

  it("names a typed account that holds none", async () => {
    const root = await drawWith(account(0n, 0n), false);
    root.querySelector<HTMLInputElement>("#plan-account")!.value = "other.testnet";
    root.querySelector("form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await vi.waitFor(() => expect(root.querySelector("#plan-result")?.textContent).toContain("other.testnet holds no BAG."));
  });

  it("keeps No limit applies for an unlimited bag with a balance", async () => {
    const root = await drawWith(account(5n * ONE, 5n * ONE));
    await vi.waitFor(() => expect(root.querySelector("#plan-result")?.textContent).toContain("No limit applies to your bag."));
    expect(root.querySelector("#plan-result")?.textContent).toContain("You can sell 5.0 BAG now.");
  });

  it("puts the result before the fields once an account is filled, and after when empty", async () => {
    const root = await drawWith(account(5n * ONE, 5n * ONE));
    const children = () => Array.from(root.children).map((c) => c.id || c.tagName.toLowerCase());
    await vi.waitFor(() => expect(root.querySelector("#plan-card")).not.toBeNull());
    expect(children().indexOf("plan-result")).toBeLessThan(children().indexOf("form"));
    const { control } = wallet();
    const empty = drawYourPlan(reads, control, vi.fn(async () => loaded));
    const kids = Array.from(empty.children).map((c) => c.id || c.tagName.toLowerCase());
    expect(kids.indexOf("form")).toBeLessThan(kids.indexOf("plan-result"));
  });
});
