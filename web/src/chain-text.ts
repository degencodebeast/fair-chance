// Text the chain gave (names, tickers, descriptions, account IDs) can hold control and direction
// characters that reorder or hide what the page shows. The page removes them where it reads the
// text, so nothing downstream sees them. A newline stays. Same rule as the bot.

const DIRECTION = /[؜‎‏‪-‮⁦-⁩]/g;
const CONTROL = /[\u0000-\u0009\u000B-\u001F\u007F-\u009F]/g;

export function cleanChainText(text: string): string {
  return text.replace(CONTROL, "").replace(DIRECTION, "");
}

/** Cleans every string in a parsed JSON value, deep. Numbers, flags and null stay as they are. */
export function cleanChainValue<T>(value: T): T {
  if (typeof value === "string") return cleanChainText(value) as T;
  if (Array.isArray(value)) return value.map(cleanChainValue) as T;
  if (typeof value === "object" && value !== null) {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, cleanChainValue(v)])) as T;
  }
  return value;
}
