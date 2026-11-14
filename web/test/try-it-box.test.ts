// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { FAUCET_URL, WALLET_NAME, TRY_IT_NOTE, tryItSteps } from "../src/try-it-copy.ts";
import { drawTryItBox } from "../src/render-try-it-box.ts";
import { drawLanding } from "../src/render-landing.ts";
import { drawCoinList } from "../src/render-coin-list.ts";
import { copyViolations } from "./copy-rules.ts";

describe("the how to try it text", () => {
  it("has three steps: a wallet, test NEAR from the official faucet, then launch or buy", () => {
    const steps = tryItSteps();
    expect(steps).toHaveLength(3);
    expect(steps[0]).toContain(WALLET_NAME);
    expect(steps[1]).toMatch(/faucet/i);
    expect(steps[2]).toMatch(/launch/i);
    expect(steps[2]).toMatch(/buy/i);
  });
  it("names the faucet in one constant: the link on the NEAR docs", () => {
    expect(FAUCET_URL).toBe("https://docs.near.org/getting-started/faucet");
  });
  it("says in one line what is true: testnet, no value, no audit", () => {
    expect(TRY_IT_NOTE).toBe("This runs on NEAR testnet. Test tokens have no value. The contracts have no audit.");
  });
  it("follows the copy rules", () => {
    for (const t of [...tryItSteps(), TRY_IT_NOTE]) expect(copyViolations(t), t).toEqual([]);
    expect([...tryItSteps(), TRY_IT_NOTE].join(" ")).not.toMatch(/production|pump|protects/i);
  });
});

describe("the how to try it box", () => {
  it("draws the steps and the faucet link", () => {
    const box = drawTryItBox();
    expect(box.id).toBe("try-it");
    expect(box.querySelectorAll("ol li")).toHaveLength(3);
    expect(box.querySelector<HTMLAnchorElement>(`a[href="${FAUCET_URL}"]`)).not.toBeNull();
    expect(box.querySelector("a")?.getAttribute("rel")).toContain("noopener");
    expect(box.textContent).toContain(TRY_IT_NOTE);
  });
  it("is on the landing screen and the coin list", () => {
    document.body.innerHTML = '<main id="app"></main>';
    drawLanding(document.querySelector("#app")!, "http://localhost:5173/");
    expect(document.querySelector("#try-it")).not.toBeNull();
    document.body.innerHTML = '<main id="app"></main>';
    drawCoinList(document.querySelector("#app")!, "http://localhost:5173/?view=coins");
    expect(document.querySelector("#try-it")).not.toBeNull();
  });
});
