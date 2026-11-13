// The plain text for every contract refusal, from the file the whole repo shares. The page never
// writes a refusal text of its own. A key is an FC_ code, or the fixed text of a standard NEP-141
// or NEP-145 message, with ACCOUNT_PLACEHOLDER where the message names an account.
import texts from "../../refusal-texts/refusal-texts.json";

const byKey: Readonly<Record<string, string>> = texts;
const ACCOUNT_PLACEHOLDER = "<account>";

/** The FC_ refusal codes the file has a text for. */
export function refusalCodes(): string[] {
  return Object.keys(byKey).filter((key) => key.startsWith("FC_"));
}

/** The plain text for a refusal key; null for a key the file does not know. */
export function refusalText(key: string): string | null {
  return byKey[key] ?? null;
}

/**
 * The plain text for a contract failure message, raw or wrapped by the chain ("Smart contract
 * panicked: ..."). An FC_ code in the message picks its text. Otherwise a standard message
 * matches by its fixed text, with any account in place of the placeholder. Null when no
 * refusal in the file matches.
 */
export function refusalTextForMessage(message: string): string | null {
  const code = message.match(/FC_[A-Z_]+/)?.[0];
  if (code) return refusalText(code);
  for (const [key, text] of Object.entries(byKey)) {
    if (!key.startsWith("FC_") && matchesStandardMessage(key, message)) return text;
  }
  return null;
}

/** Whether the message holds the key, where the key's placeholder stands for one account ID. */
function matchesStandardMessage(key: string, message: string): boolean {
  const at = key.indexOf(ACCOUNT_PLACEHOLDER);
  if (at < 0) return message.includes(key);
  const before = key.slice(0, at);
  const after = key.slice(at + ACCOUNT_PLACEHOLDER.length);
  const start = message.indexOf(before);
  if (start < 0) return false;
  const rest = message.slice(start + before.length);
  const account = rest.match(/^[^\s"']*/)![0];
  return account !== "" && rest.slice(account.length).startsWith(after);
}
