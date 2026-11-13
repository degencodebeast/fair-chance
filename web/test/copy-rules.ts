// The copy rules of the design requirements, as a check on any text the page shows.
const BANNED_WORDS = ["safe", "rug-proof", "audited", "guaranteed", "first", "only", "AI", "queued", "pending", "waits", "17 seconds", "18 seconds", "17-second", "18-second"];
const EMOJI = /\p{Extended_Pictographic}/u;

/** The rule each piece of text breaks; empty when the text is clean. */
export function copyViolations(text: string): string[] {
  const found: string[] = [];
  for (const word of BANNED_WORDS) {
    // "AI" is matched as written; the other words in any case.
    const flags = word === "AI" ? "" : "i";
    // A space in a banned phrase matches any white space, including a no-break space and several spaces.
    const phrase = word.replace(/ /g, "\\s+");
    if (new RegExp(`(^|[^A-Za-z-])${phrase}([^A-Za-z-]|$)`, flags).test(text)) found.push(`banned word "${word}"`);
  }
  // The early window (blocks 0 to 17) is about 10 seconds at the measured mainnet block time; give a time only in that form.
  for (const match of text.matchAll(/about\s+10\s+seconds/gi)) {
    const after = text.slice((match.index ?? 0) + match[0].length);
    if (!/^\s+at\s+the\s+measured\s+mainnet\s+block\s+time/i.test(after)) {
      found.push("early window time without the measured mainnet block time");
      break;
    }
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
