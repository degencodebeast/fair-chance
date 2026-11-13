import { describe, expect, it } from "vitest";
import { parseLog } from "../src/chain.ts";
import { cleanChainText } from "../src/chain-text.ts";
import { parseMetadata, parseRules } from "../src/coin-views.ts";
import { recordedCoin } from "./recorded-coin.ts";

describe("cleanChainText", () => {
  it("removes direction characters", () => {
    expect(cleanChainText("a‮b‏c⁦d⁧e⁨f⁩g‪h‬i‎j؜k")).toBe("abcdefghijk");
  });
  it("removes C0 and C1 controls but keeps a newline", () => {
    expect(cleanChainText("a\u0000b\u0007c\u001Bd\u007Fe\u0085f\u009Fg\nh\ti")).toBe("abcdefg\nhi");
  });
  it("keeps ordinary text, accents, emoji and a joiner inside an emoji", () => {
    const text = "Moth Lamp, café 日本 👩‍💻 100%";
    expect(cleanChainText(text)).toBe(text);
  });
});

describe("chain text shown by the page", () => {
  it("cleans the coin name, symbol, description and creator read from the views", () => {
    const rec = recordedCoin("normal");
    const m = parseMetadata({ ...(rec.views.ft_metadata as object), name: "Moth‮ Lamp", symbol: "MO‏TH" });
    expect([m.name, m.symbol]).toEqual(["Moth Lamp", "MOTH"]);
    const r = parseRules({ ...(rec.views.fc_rules as object), description: "a ⁦lamp⁩", creator: "lamp‮.testnet" });
    expect([r.description, r.creator]).toEqual(["a lamp", "lamp.testnet"]);
  });

  it("cleans the account IDs and text inside event data", () => {
    const ev = parseLog(`EVENT_JSON:${JSON.stringify({ standard: "fair_chance", version: "1", event: "fc_sell", data: [{ account: "al‮ice.testnet", nested: { to: "b‏ob.testnet" }, block: 5 }] })}`);
    expect(ev?.data).toEqual({ account: "alice.testnet", nested: { to: "bob.testnet" }, block: 5 });
  });
});
