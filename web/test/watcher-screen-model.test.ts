import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { watcherScreenModel } from "../src/watcher-screen-model.ts";
import { allText, copyViolations } from "./copy-rules.ts";

const file = JSON.parse(readFileSync(join(import.meta.dirname, "../../testdata/watcher-sample-messages/messages.json"), "utf8"));
const bare = "http://localhost/?view=alerts&network=testnet";
const withCoin = "http://localhost/?view=alerts&network=testnet&coin=real.factory.testnet";

describe("the alerts screen", () => {
  it("shows the chat in order: /watch, its reply, each alert, /check, its reply", () => {
    const m = watcherScreenModel(bare);
    expect(m.chat.map((c) => c.from)).toEqual(["you", "bot", ...file.alerts.map(() => "bot"), "you", "bot"]);
    expect(m.chat[0]!.text).toBe(file.watch_command);
    expect(m.chat[1]!.text).toBe(file.watch_reply);
    expect(m.chat.slice(2, -2).map((c) => c.text)).toEqual(file.alerts.map((a: { text: string }) => a.text));
    expect(m.chat.at(-1)!.text).toBe(file.check_reply);
  });

  it("shows in each bot bubble only the text the bot sends, with no heading of the page's own", () => {
    const m = watcherScreenModel(bare);
    expect(m.chat.filter((c) => c.from === "bot").map((c) => [c.head, c.text])).toEqual([
      [null, file.watch_reply],
      ...file.alerts.map((a: { text: string }) => [null, a.text]),
      [null, file.check_reply],
    ]);
  });

  it("uses the alert sentences from the watcher templates and does not write its own", () => {
    const m = watcherScreenModel(bare);
    expect(m.plain.map((p) => p.text)).toEqual([file.watch_reply, ...file.alerts.map((a: { text: string }) => a.text), file.check_reply]);
    expect(m.checkCard).toBe(file.check_reply);
  });

  it("labels the chat card, the plain text card and the pre-buy card as sample data, each on its own", () => {
    const m = watcherScreenModel(bare);
    expect(m.sampleLabel).toMatch(/^Sample data/);
    expect(m.chatLabel).toMatch(/^Sample data/);
    expect(m.plainLabel).toMatch(/^Sample data/);
    expect(m.preBuyLabel).toMatch(/^Sample data/);
  });

  it("keeps Open on Fair Chance off for sample alerts even when the page link names a coin, because no sample receipt exists on chain", () => {
    for (const href of [bare, withCoin]) {
      const m = watcherScreenModel(href);
      expect(m.chat.every((c) => c.openHref === null)).toBe(true);
      expect(m.openNote).toMatch(/no real receipt/);
    }
  });

  it("follows the copy rules", () => {
    const m = watcherScreenModel(withCoin);
    for (const text of allText(m).filter((t) => !t.includes("\n"))) expect(copyViolations(text), text).toEqual([]);
  });
});
