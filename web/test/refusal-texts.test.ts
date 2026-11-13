import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { refusalCodes, refusalText } from "../src/refusal-texts.ts";
import { copyViolations } from "./copy-rules.ts";

const interfaceFile = readFileSync(join(import.meta.dirname, "../../docs/INTERFACE.md"), "utf8");
// FC_TRANSFER_CALL_NOT_POOL is the retired name of FC_TRANSFER_CALL.
const interfaceCodes = [...new Set(interfaceFile.match(/FC_[A-Z_]+/g) ?? [])].filter(
  (code) => code !== "FC_TRANSFER_CALL_NOT_POOL",
);

describe("refusal texts", () => {
  it("has a text for every refusal code in the interface", () => {
    expect(interfaceCodes.length).toBeGreaterThan(8);
    for (const code of interfaceCodes) expect(refusalText(code), code).toEqual(expect.any(String));
  });

  it("has no text for a code the interface does not name", () => {
    expect(refusalCodes().filter((code) => !interfaceCodes.includes(code))).toEqual([]);
    expect(refusalText("FC_NO_SUCH_CODE")).toBeNull();
  });

  it("follows the copy rules", () => {
    for (const code of refusalCodes()) expect(copyViolations(refusalText(code)!), code).toEqual([]);
  });
});
