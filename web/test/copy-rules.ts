// The copy rules of the design requirements, as a check on any text the page shows.
const BANNED_WORDS = ["safe", "rug-proof", "audited", "guaranteed", "first", "only", "AI", "queued", "pending", "waits"];
const EMOJI = /\p{Extended_Pictographic}/u;

/** The rule each piece of text breaks; empty when the text is clean. */
export function copyViolations(text: string): string[] {
  const found: string[] = [];
  for (const word of BANNED_WORDS) {
    // "AI" is matched as written; the other words in any case.
    const flags = word === "AI" ? "" : "i";
    if (new RegExp(`(^|[^A-Za-z-])${word}([^A-Za-z-]|$)`, flags).test(text)) found.push(`banned word "${word}"`);
  }
  if (text.includes("—")) found.push("em dash");
  if (EMOJI.test(text)) found.push("emoji");
  return found;
}

/** Every string in a value, deep. */
export function allText(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (Array.isArray(value)) return value.flatMap(allText);
  if (typeof value === "object" && value !== null) return Object.values(value).flatMap(allText);
  return [];
}
