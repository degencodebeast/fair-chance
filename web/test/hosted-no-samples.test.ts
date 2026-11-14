// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { fallbackList } from "../src/list-fallback.ts";
import { SAMPLE_COINS } from "../src/coin-list-sample.ts";
import { resolveListLink } from "../src/page-link.ts";
import { drawLanding } from "../src/render-landing.ts";
import { drawAlertsScreen } from "../src/render-alerts-screen.ts";
import { drawLaunchForm } from "../src/render-launch-form.ts";

describe("the coin list with nothing to read", () => {
  it("keeps the made-up coins on the local machine", () => {
    expect(fallbackList({ kind: "sample" }, "localhost").cards).toBe(SAMPLE_COINS);
    expect(fallbackList({ kind: "no-factory", source: "NEAR testnet" }, "127.0.0.1").cards).toBe(SAMPLE_COINS);
  });
  it("shows no made-up coin on a hosted host, and says why", () => {
    for (const link of [{ kind: "sample" }, { kind: "no-factory", source: "NEAR testnet" }] as const) {
      const out = fallbackList(link, "fc.example");
      expect(out.cards).toEqual([]);
      expect(out.note).not.toMatch(/sample|made up/i);
    }
  });
});

describe("a hosted link with no network", () => {
  it("reads the testnet chain, not sample data", () => {
    const link = resolveListLink("https://fc.example/?view=coins");
    expect(link.kind).toBe("chain");
  });
  it("still gives sample data on the local machine", () => {
    expect(resolveListLink("http://localhost:5173/?view=coins").kind).toBe("sample");
  });
});

describe("the landing screen", () => {
  const text = (href: string) => {
    document.body.innerHTML = '<main id="app"></main>';
    drawLanding(document.querySelector("#app")!, href);
    return document.body.textContent ?? "";
  };
  it("has no sample data label on a hosted host", () => {
    expect(document.body.innerHTML).toBeDefined();
    const t = text("https://fc.example/?network=testnet");
    expect(t).not.toMatch(/sample data/i);
    expect(document.querySelector("#proof .proof-source")?.textContent).toMatch(/NEAR testnet/);
  });
  it("links the footer to the project on GitHub", () => {
    text("https://fc.example/?network=testnet");
    const link = document.querySelector<HTMLAnchorElement>(".landing-footer a")!;
    expect(link.textContent).toBe("GitHub");
    expect(link.getAttribute("href")).toBe("https://github.com/degencodebeast/fair-chance");
  });
  it("shows the same real testnet result on the local machine, with no sample label", () => {
    const t = text("http://localhost:5173/");
    expect(t).not.toMatch(/sample data/i);
    expect(t).toContain("2.98M sold, 7.5% of what they asked for");
  });
});

describe("every screen on a hosted host", () => {
  const draw = (screen: "landing" | "alerts" | "launch", href: string) => {
    document.body.innerHTML = '<main id="app"></main>';
    const app = document.querySelector<HTMLElement>("#app")!;
    if (screen === "landing") drawLanding(app, href);
    else if (screen === "alerts") drawAlertsScreen(app, href);
    else drawLaunchForm(app, href);
    return document.body.textContent ?? "";
  };
  it.each(["landing", "alerts", "launch"] as const)("%s shows no made-up value", (screen) => {
    const text = draw(screen, `https://fc.example/?network=testnet${screen === "alerts" ? "&view=alerts" : ""}`);
    expect(text).not.toMatch(/sample|made.up|\.sample\b/i);
    expect(document.querySelector(".sample-label, .chat, #pre-buy-card")).toBeNull();
  });
  it("alerts links to the bot with its sentence, and to the coin list", () => {
    draw("alerts", "https://fc.example/?view=alerts&network=testnet");
    expect(document.body.textContent).toMatch(/Telegram/);
    expect(document.querySelector<HTMLAnchorElement>("#alerts-coin-link")?.getAttribute("href")).toBe("?view=coins&network=testnet");
    const bot = document.querySelector<HTMLAnchorElement>("#alerts-bot-link");
    expect(bot?.getAttribute("href")).toBe("https://t.me/fairchance_alert_bot");
    expect(bot?.getAttribute("rel")).toBe("noopener noreferrer");
    expect(document.body.textContent).toContain("Send /check <coin> for its rules. Alerts go to the coin's team only for now.");
  });
  it("alerts links to the named coin page when the link names a coin", () => {
    draw("alerts", "https://fc.example/?view=alerts&network=testnet&coin=a.factory.fairchance.testnet");
    expect(document.querySelector<HTMLAnchorElement>("#alerts-coin-link")?.getAttribute("href")).toBe("?coin=a.factory.fairchance.testnet&network=testnet");
  });
  it("alerts keeps the made-up chat on the local machine", () => {
    draw("alerts", "http://localhost:5173/?view=alerts");
    expect(document.querySelector(".chat")).not.toBeNull();
  });
});
