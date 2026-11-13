// The plain text for every FC_ refusal code, from the file the whole repo shares. The page never
// writes a refusal text of its own.
import texts from "../../refusal-texts/refusal-texts.json";

const byCode: Readonly<Record<string, string>> = texts;

export function refusalCodes(): string[] {
  return Object.keys(byCode);
}

/** The plain text for a refusal code; null for a code the file does not know. */
export function refusalText(code: string): string | null {
  return byCode[code] ?? null;
}
