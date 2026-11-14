// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { walletBar } from "../src/screen-chrome.ts";
import { walletControl, type WalletAccount, type WalletLibrary } from "../src/wallet-session.ts";
import { expectNoEnabledControlWithoutHandler } from "./dom-controls.ts";

const PAGE = "https://example.test/?network=testnet&view=coins";

function library(account: WalletAccount, gate?: Promise<void>): WalletLibrary & { signOuts: number } {
  const lib = {
    signOuts: 0,
    async connect() {
      await gate;
      return account;
    },
    restore: async () => null,
    send: async () => [],
    async signOut() {
      lib.signOuts++;
    },
    onSignIn: () => {},
    onSignOut: () => {},
  };
  return lib;
}

const flush = () => new Promise((r) => setTimeout(r, 0));
const button = (root: HTMLElement, id: string) => root.querySelector<HTMLButtonElement>(`#${id}`);

describe("wallet bar", () => {
  it("connects on click, shows the account and signs out", async () => {
    const lib = library({ accountId: "lamp.testnet", network: "testnet" });
    const bar = walletBar(walletControl(PAGE, () => lib));
    document.body.replaceChildren(bar);
    expect(bar.id).toBe("wallet-bar");
    expect(button(bar, "wallet-connect")?.textContent).toBe("Connect wallet");
    expectNoEnabledControlWithoutHandler(bar);

    button(bar, "wallet-connect")!.click();
    await flush();
    expect(bar.querySelector("#wallet-account")?.textContent).toBe("lamp.testnet");
    expect(button(bar, "wallet-connect")).toBeNull();
    expect(button(bar, "wallet-sign-out")?.textContent).toBe("Sign out");
    expectNoEnabledControlWithoutHandler(bar);

    button(bar, "wallet-sign-out")!.click();
    await flush();
    expect(lib.signOuts).toBe(1);
    expect(bar.querySelector("#wallet-account")).toBeNull();
    expect(button(bar, "wallet-connect")).not.toBeNull();
  });

  it("turns the button off while the wallet is open", async () => {
    let open!: () => void;
    const gate = new Promise<void>((r) => (open = r));
    const bar = walletBar(walletControl(PAGE, () => library({ accountId: "lamp.testnet", network: "testnet" }, gate)));
    button(bar, "wallet-connect")!.click();
    expect(button(bar, "wallet-connect")?.disabled).toBe(true);
    expect(button(bar, "wallet-connect")?.textContent).toBe("Connecting");
    open();
    await flush();
    expect(bar.querySelector("#wallet-account")?.textContent).toBe("lamp.testnet");
  });

  it("shows the one refusal sentence for a mainnet account", async () => {
    const bar = walletBar(walletControl(PAGE, () => library({ accountId: "lamp.near", network: "testnet" })));
    button(bar, "wallet-connect")!.click();
    await flush();
    expect(bar.querySelector("#wallet-notice")?.textContent).toBe("This page works only with NEAR testnet accounts, so it signed out lamp.near.");
    expect(bar.querySelector("#wallet-notice")?.getAttribute("role")).toBe("status");
    expect(bar.querySelector("#wallet-account")).toBeNull();
  });

  it("draws the state the control already has", async () => {
    const control = walletControl(PAGE, () => library({ accountId: "lamp.testnet", network: "testnet" }));
    await control.connect();
    expect(walletBar(control).querySelector("#wallet-account")?.textContent).toBe("lamp.testnet");
  });
});
