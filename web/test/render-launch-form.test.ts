// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { LAUNCH_LIMITS } from "../src/launch-form-model.ts";
import type { LaunchArgs } from "../src/launch-form-model.ts";
import type { IconResult } from "../src/icon-to-webp.ts";
import { drawLaunchForm, type LaunchSend } from "../src/render-launch-form.ts";
import type { LaunchedCoin } from "../src/launch-handoff.ts";
import type { TradeOutcome } from "../src/trade-actions.ts";
import { walletControl } from "../src/wallet-session.ts";
import { expectNoEnabledControlWithoutHandler, turnedOffReasons } from "./dom-controls.ts";

function form() {
  const app = document.createElement("main");
  document.body.replaceChildren(app);
  drawLaunchForm(app, "http://localhost/?view=launch");
  return app;
}
const type = (app: HTMLElement, id: string, value: string) => {
  const input = app.querySelector<HTMLInputElement>(id)!;
  input.value = value;
  input.dispatchEvent(new Event("input"));
};

describe("launch form", () => {
  it("gives each field its length limit, the description the same as the model", () => {
    const app = form();
    expect(app.querySelector("#launch-description")!.getAttribute("maxlength")).toBe(String(LAUNCH_LIMITS.description));
    expect(app.querySelector("#launch-name")!.getAttribute("maxlength")).toBe(String(LAUNCH_LIMITS.name));
    expect(app.querySelector("#launch-ticker")!.getAttribute("maxlength")).toBe(String(LAUNCH_LIMITS.ticker));
  });

  it("has no enabled control without a handler", () => {
    expectNoEnabledControlWithoutHandler(form());
  });

  it("turns off the launch button until a wallet connects, with a reason", () => {
    const reasons = turnedOffReasons(form());
    expect(Object.keys(reasons)).toEqual(["launch-button"]);
    expect(reasons["launch-button"]).toBe("Connect a wallet to launch.");
  });

  it("shows the total NEAR and that the deposit stays in the coin account, beside the button", () => {
    const app = form();
    type(app, "#launch-buy", "4.4");
    expect(app.querySelector("#launch-deposit")?.textContent).toBe("You attach 4.65 NEAR: a 0.25 NEAR deposit and a 4.4 NEAR opening buy. The deposit stays in the coin account. Nobody can take it out.");
  });

  it("updates the count, the preview and the button label as the visitor types", () => {
    const app = form();
    type(app, "#launch-name", "Moth Lamp");
    type(app, "#launch-ticker", "moth");
    type(app, "#launch-description", "A lamp for moths.");
    expect(app.querySelector("#launch-name-count")?.textContent).toBe("9/32");
    expect(app.querySelector("#launch-preview")?.textContent).toContain("Moth Lamp");
    expect(app.querySelector("#launch-preview")?.textContent).toContain("MOTH");
    expect(app.querySelector("#launch-button")?.textContent).toBe("Launch Moth Lamp");
  });

  it("shows an error for a field that breaks a limit", () => {
    const app = form();
    type(app, "#launch-name", "Moth Lamp");
    type(app, "#launch-ticker", "MO TH");
    expect(app.querySelector("#launch-errors")?.textContent).toContain("letters, digits, - and _ only");
  });

  it("does not submit anything: the form handler stops the default", () => {
    const app = form();
    const event = new Event("submit", { cancelable: true });
    app.querySelector("#launch-form")!.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });
});

describe("launch form name check", () => {
  function checked(answer: () => Promise<"free" | "taken">, factory: string | null = "factory.sandbox") {
    const app = document.createElement("main");
    document.body.replaceChildren(app);
    const calls: string[] = [];
    const checkName = factory === null ? null : async (id: string) => { calls.push(id); return answer(); };
    drawLaunchForm(app, "http://localhost/?view=launch", { factory, checkName });
    return { app, calls };
  }
  const change = (app: HTMLElement, value: string) => {
    const t = app.querySelector<HTMLInputElement>("#launch-ticker")!;
    t.value = value;
    t.dispatchEvent(new Event("input"));
    t.dispatchEvent(new Event("change"));
  };
  const text = (app: HTMLElement) => app.querySelector("#launch-name-check")?.textContent ?? "";

  it("shows free for a free account and checks the lowered ticker under the factory", async () => {
    const { app, calls } = checked(async () => "free");
    change(app, "Bag");
    await vi.waitFor(() => expect(text(app)).toBe("bag.factory.sandbox is free."));
    expect(calls).toEqual(["bag.factory.sandbox"]);
  });

  it("shows taken", async () => {
    const { app } = checked(async () => "taken");
    change(app, "bag");
    await vi.waitFor(() => expect(text(app)).toBe("bag.factory.sandbox is taken. Pick another ticker."));
  });

  it("says it did not check when the read fails, and never shows free", async () => {
    const { app } = checked(async () => { throw new Error("rpc down"); });
    change(app, "bag");
    await vi.waitFor(() => expect(text(app)).toMatch(/^Not checked/));
    expect(text(app)).not.toMatch(/free/);
  });

  it("says it cannot check when the link names no factory", () => {
    const { app } = checked(async () => "free", null);
    change(app, "bag");
    expect(text(app)).toBe("Not checked: this link names no factory.");
  });

  it("does not read for a ticker that breaks a rule", () => {
    const { app, calls } = checked(async () => "free");
    change(app, "$BAG");
    expect(calls).toEqual([]);
    expect(text(app)).toBe("");
  });

  it("drops an older answer that arrives after a newer one", async () => {
    const waiting: ((v: "free" | "taken") => void)[] = [];
    const { app } = checked(() => new Promise((r) => { waiting.push(r); }));
    change(app, "one");
    change(app, "two");
    waiting[1]!("taken");
    await vi.waitFor(() => expect(text(app)).toBe("two.factory.sandbox is taken. Pick another ticker."));
    waiting[0]!("free");
    await new Promise((r) => setTimeout(r, 20));
    expect(text(app)).toBe("two.factory.sandbox is taken. Pick another ticker.");
  });

  it("keeps the launch button turned off whatever the answer", async () => {
    const { app } = checked(async () => "free");
    change(app, "bag");
    await vi.waitFor(() => expect(text(app)).toMatch(/free/));
    expect(app.querySelector<HTMLButtonElement>("#launch-button")!.disabled).toBe(true);
    expectNoEnabledControlWithoutHandler(app);
  });
});

describe("launch form name check, stale answers", () => {
  const setup = (check: (id: string) => Promise<"free" | "taken">) => {
    const app = document.createElement("main");
    document.body.replaceChildren(app);
    drawLaunchForm(app, "http://localhost/?view=launch", { factory: "factory.sandbox", checkName: check });
    return app;
  };
  const typeTicker = (app: HTMLElement, value: string, change: boolean) => {
    const t = app.querySelector<HTMLInputElement>("#launch-ticker")!;
    t.value = value;
    t.dispatchEvent(new Event("input"));
    if (change) t.dispatchEvent(new Event("change"));
  };
  const text = (app: HTMLElement) => app.querySelector("#launch-name-check")?.textContent ?? "";

  it("drops an old failure that arrives after a newer answer", async () => {
    const waiting: { resolve: (v: "free" | "taken") => void; reject: (e: Error) => void }[] = [];
    const app = setup(() => new Promise((resolve, reject) => { waiting.push({ resolve, reject }); }));
    typeTicker(app, "one", true);
    typeTicker(app, "two", true);
    waiting[1]!.resolve("free");
    await vi.waitFor(() => expect(text(app)).toBe("two.factory.sandbox is free."));
    waiting[0]!.reject(new Error("rpc down"));
    await new Promise((r) => setTimeout(r, 20));
    expect(text(app)).toBe("two.factory.sandbox is free.");
  });

  it("clears the answer as soon as the ticker is typed again, before any change event", async () => {
    const app = setup(async () => "free");
    typeTicker(app, "one", true);
    await vi.waitFor(() => expect(text(app)).toBe("one.factory.sandbox is free."));
    typeTicker(app, "onetwo", false);
    expect(text(app)).toBe("");
  });

  it("drops an answer that arrives after the ticker was typed again", async () => {
    let release: (v: "free" | "taken") => void = () => undefined;
    const app = setup(() => new Promise((r) => { release = r; }));
    typeTicker(app, "one", true);
    typeTicker(app, "onex", false);
    release("free");
    await new Promise((r) => setTimeout(r, 20));
    expect(text(app)).toBe("");
  });
});

describe("launch form picture", () => {
  const icon = "data:image/webp;base64,UklGRg==";
  function withPicture(convert: (f: File) => Promise<IconResult>) {
    const app = document.createElement("main");
    document.body.replaceChildren(app);
    const seen: Array<LaunchArgs | null> = [];
    drawLaunchForm(app, "http://localhost/?view=launch", { factory: null, checkName: null }, { convert, onArgs: (a) => seen.push(a) });
    return { app, seen };
  }
  const choose = (app: HTMLElement, name = "pic.png") => {
    const input = app.querySelector<HTMLInputElement>("#launch-picture-input")!;
    Object.defineProperty(input, "files", { value: [new File(["x"], name, { type: "image/png" })], configurable: true });
    input.dispatchEvent(new Event("change"));
  };
  const status = (app: HTMLElement) => app.querySelector("#launch-picture-status")?.textContent ?? "";

  it("has a file input that takes images, with a handler", () => {
    const { app } = withPicture(async () => ({ ok: true, dataUrl: icon }));
    const input = app.querySelector<HTMLInputElement>("#launch-picture-input")!;
    expect(input.type).toBe("file");
    expect(input.accept).toMatch(/image\//);
    expectNoEnabledControlWithoutHandler(app);
  });

  it("shows the converted picture and passes it into the launch arguments", async () => {
    const { app, seen } = withPicture(async () => ({ ok: true, dataUrl: icon }));
    type(app, "#launch-name", "Moth Lamp");
    type(app, "#launch-ticker", "moth");
    choose(app);
    await vi.waitFor(() => expect(app.querySelector<HTMLImageElement>("#launch-picture-preview")?.getAttribute("src")).toBe(icon));
    expect(seen.at(-1)).toMatchObject({ name: "Moth Lamp", ticker: "moth", icon });
    expect(status(app)).toMatch(/KB|characters/);
  });

  it("shows the refusal sentence, no preview and no icon when the picture cannot fit", async () => {
    const { app, seen } = withPicture(async () => ({ ok: false, reason: "This picture does not fit in 8 KB, even at 64 px. Choose a simpler picture." }));
    type(app, "#launch-name", "Moth Lamp");
    type(app, "#launch-ticker", "moth");
    choose(app);
    await vi.waitFor(() => expect(status(app)).toContain("does not fit in 8 KB"));
    expect(app.querySelector("#launch-picture-preview")).toBeNull();
    expect(seen.at(-1)).toBeNull();
    expect(app.querySelector("#launch-errors")?.textContent).toContain("does not fit in 8 KB");
  });

  it("gives no launch arguments while the picture is being shrunk", () => {
    const { app, seen } = withPicture(() => new Promise(() => undefined));
    type(app, "#launch-name", "Moth Lamp");
    type(app, "#launch-ticker", "moth");
    choose(app);
    expect(seen.at(-1)).toBeNull();
    expect(app.querySelector("#launch-errors")?.textContent).toContain("still being shrunk");
  });

  it("gives no launch arguments when the conversion throws", async () => {
    const { app, seen } = withPicture(async () => { throw new Error("boom"); });
    type(app, "#launch-name", "Moth Lamp");
    type(app, "#launch-ticker", "moth");
    choose(app);
    await vi.waitFor(() => expect(status(app)).toMatch(/not read this file/));
    expect(seen.at(-1)).toBeNull();
    expect(app.querySelector("#launch-errors")?.textContent).toMatch(/not read this file/);
  });

  it("gives valid arguments with no icon after the file is cleared", async () => {
    const { app, seen } = withPicture(async () => ({ ok: true, dataUrl: icon }));
    type(app, "#launch-name", "Moth Lamp");
    type(app, "#launch-ticker", "moth");
    choose(app);
    await vi.waitFor(() => expect(seen.at(-1)?.icon).toBe(icon));
    const input = app.querySelector<HTMLInputElement>("#launch-picture-input")!;
    Object.defineProperty(input, "files", { value: [], configurable: true });
    input.dispatchEvent(new Event("change"));
    expect(seen.at(-1)).toMatchObject({ name: "Moth Lamp", icon: null });
  });

  it("drops an older conversion that finishes after a newer choice", async () => {
    const waiting: Array<(r: IconResult) => void> = [];
    const { app } = withPicture(() => new Promise((r) => { waiting.push(r); }));
    choose(app, "one.png");
    choose(app, "two.png");
    waiting[1]!({ ok: true, dataUrl: "data:image/webp;base64,BBBB" });
    await vi.waitFor(() => expect(app.querySelector("#launch-picture-preview")?.getAttribute("src")).toBe("data:image/webp;base64,BBBB"));
    waiting[0]!({ ok: true, dataUrl: "data:image/webp;base64,AAAA" });
    await new Promise((r) => setTimeout(r, 20));
    expect(app.querySelector("#launch-picture-preview")?.getAttribute("src")).toBe("data:image/webp;base64,BBBB");
  });

  it("keeps the typed fields when the picture is chosen", async () => {
    const { app } = withPicture(async () => ({ ok: true, dataUrl: icon }));
    type(app, "#launch-name", "Moth Lamp");
    choose(app);
    await vi.waitFor(() => expect(app.querySelector("#launch-preview img")).not.toBeNull());
    expect(app.querySelector<HTMLInputElement>("#launch-name")!.value).toBe("Moth Lamp");
  });
});

describe("a launch from the form", () => {
  const launched: TradeOutcome = {
    kind: "result",
    result: { kind: "launched", txHash: "Hx3", explorerUrl: "https://testnet.nearblocks.io/txns/Hx3", coin: "moth.factory.sandbox", creator: "you.testnet", launchBlock: 1234, description: null, openingTokens: 0n },
  };
  async function signedInForm(send: LaunchSend["launch"]) {
    const wallet = walletControl("https://example.test/?network=testnet&view=launch", () => ({
      connect: async () => ({ accountId: "you.testnet", network: "testnet" }),
      restore: async () => null,
      send: async () => [],
      signOut: async () => {},
      onSignIn: () => {},
      onSignOut: () => {},
    }));
    await wallet.connect();
    const seen: LaunchedCoin[] = [];
    const app = document.createElement("main");
    document.body.replaceChildren(app);
    const launch = vi.fn(send);
    drawLaunchForm(app, "http://localhost/?view=launch", { factory: "factory.sandbox", checkName: null }, {}, { wallet, launch, onLaunched: (c) => seen.push(c) });
    type(app, "#launch-name", "Moth Lamp");
    type(app, "#launch-ticker", "moth");
    return { app, seen, launch };
  }

  it("turns the button on, waits for the wallet, then hands the new coin to its page", async () => {
    let answer!: (o: TradeOutcome) => void;
    const { app, seen, launch } = await signedInForm(() => new Promise((r) => (answer = r)));
    const button = () => app.querySelector<HTMLButtonElement>("#launch-button")!;
    expect(button().disabled).toBe(false);
    expectNoEnabledControlWithoutHandler(app);
    button().click();
    expect(button().disabled).toBe(true);
    expect(button().textContent).toBe("Waiting for the wallet");
    expect(launch).toHaveBeenCalledWith(expect.objectContaining({ accountId: "you.testnet" }), "factory.sandbox", expect.objectContaining({ name: "Moth Lamp", ticker: "moth" }), null);
    answer(launched);
    await vi.waitFor(() => expect(seen).toHaveLength(1));
    expect(seen[0]).toEqual({ coin: "moth.factory.sandbox", name: "Moth Lamp", ticker: "MOTH", icon: null, creator: "you.testnet", launchBlock: 1234, explorerUrl: "https://testnet.nearblocks.io/txns/Hx3" });
  });

  it("shows the sentence and turns the button back on when the wallet closes", async () => {
    const { app, seen } = await signedInForm(async () => ({ kind: "not-sent", text: "The wallet closed and sent nothing." }));
    app.querySelector<HTMLButtonElement>("#launch-button")!.click();
    await vi.waitFor(() => expect(app.querySelector("#launch-result")?.textContent).toBe("The wallet closed and sent nothing."));
    expect(app.querySelector<HTMLButtonElement>("#launch-button")!.disabled).toBe(false);
    expect(seen).toHaveLength(0);
  });
});
