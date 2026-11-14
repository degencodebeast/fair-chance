// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { drawRecordedRun } from "../src/render-recorded-run.ts";

describe("the Recorded testnet run section", () => {
  it("is labelled as recorded, with one row and one explorer link per transaction", () => {
    const section = drawRecordedRun({ ok: true, rows: [{ txHash: "h1", link: "https://x.example/txns/h1", text: "a sold some." }, { txHash: "h2", link: "https://x.example/txns/h2", text: "b was refused." }] });
    expect(section.id).toBe("recorded-run");
    expect(section.querySelector("h2")?.textContent).toBe("Recorded testnet run");
    expect(section.textContent).toContain("Recorded on testnet. Not read from the chain now.");
    const rows = [...section.querySelectorAll("li[data-tx-hash]")];
    expect(rows.map((r) => r.getAttribute("data-tx-hash"))).toEqual(["h1", "h2"]);
    const link = rows[0]!.querySelector("a")!;
    expect(link.getAttribute("href")).toBe("https://x.example/txns/h1");
    expect(link.getAttribute("rel")).toBe("noopener noreferrer");
  });

  it("says plainly that the run cannot load, and shows no rows", () => {
    const section = drawRecordedRun({ ok: false, reason: "the file is for another coin" });
    expect(section.querySelectorAll("li")).toHaveLength(0);
    expect(section.querySelector("#recorded-run-error")?.textContent).toContain("The recorded run cannot load");
  });
});
