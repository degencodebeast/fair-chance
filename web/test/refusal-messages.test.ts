import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { refusalText, refusalTextForMessage } from "../src/refusal-texts.ts";
import { copyViolations } from "./copy-rules.ts";

// Written by the rule engine test: every refusal message the contract can return.
const contractRefusals: { refusal: string; message: string }[] = JSON.parse(
  readFileSync(join(import.meta.dirname, "../../refusal-texts/every-refusal-message.json"), "utf8"),
);

describe("refusal text for a contract failure message", () => {
  it("gives a plain sentence for every refusal the contract can return, raw or wrapped by the chain", () => {
    expect(contractRefusals.length).toBeGreaterThanOrEqual(15);
    for (const { refusal, message } of contractRefusals) {
      for (const text of [message, `Smart contract panicked: ${message}`]) {
        const sentence = refusalTextForMessage(text);
        expect(sentence, `${refusal}: ${text}`).toEqual(expect.any(String));
        expect(copyViolations(sentence!), refusal).toEqual([]);
      }
    }
  });

  it("matches a not registered refusal for any account, in both NEP-145 forms", () => {
    const registered = refusalText("The account is not registered: <account>");
    const withdraw = refusalText("The account <account> is not registered");
    expect(registered).toEqual(expect.any(String));
    expect(withdraw).toEqual(expect.any(String));
    expect(refusalTextForMessage("Smart contract panicked: The account is not registered: alice.testnet")).toBe(registered);
    expect(refusalTextForMessage("The account bob.near is not registered")).toBe(withdraw);
    expect(refusalTextForMessage("The account doesn't have enough balance")).not.toBe(registered);
  });

  it("gives no sentence for a message with no known refusal", () => {
    for (const message of ["Exceeded the prepaid gas", "FC_NO_SUCH_CODE", "The account bob.near"]) {
      expect(refusalTextForMessage(message), message).toBeNull();
    }
  });
});
