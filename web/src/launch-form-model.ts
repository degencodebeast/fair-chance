// The text and checks of the launch form. Pure. The launch button turns on for a signed-in testnet
// account with valid fields; the visitor's wallet signs, never the page.
import { exactNear } from "./format.ts";
import { iconFits } from "./icon-to-webp.ts";
import { parseTokenAmount } from "./token-amount.ts";

/**
 * Field lengths of the form, from the v5 design. The coin and the factory set no limit of their
 * own (QUESTIONS.md, launch form); until they do, this is the one place the lengths live.
 */
export const LAUNCH_LIMITS = { name: 32, ticker: 8, description: 80 } as const;

const NEAR_DECIMALS = 24;

/**
 * What the factory accepts: it lowers the ticker and uses it as the first part of the coin account, so a dot, a leading or trailing separator and two separators in a row are out.
 * The ticker is tested as typed, ASCII only: a letter such as the Kelvin sign lowers to "k" and would pass a test of the lowered text, then go on chain as the symbol, a look-alike of K.
 */
const TICKER_PATTERN = /^[A-Za-z0-9]+([-_][A-Za-z0-9]+)*$/;

export interface LaunchFormInput {
  name: string;
  ticker: string;
  description: string;
  openingBuy: string;
  /** The picture as a WebP data URL from `iconToWebp`; absent or null for no picture. */
  icon?: string | null;
  /** The state of the chosen file. A launch waits for "ready" or "none"; a pending or refused picture blocks it. */
  picture?: { state: "none" } | { state: "pending" } | { state: "ready" } | { state: "refused"; reason: string };
  /** The signed-in testnet account; absent or null when no wallet is connected. */
  signedIn?: string | null;
  /** True while the wallet is open for the launch. */
  pending?: boolean;
  /** The NEAR the launch attaches for the coin's storage, on top of the opening buy. Absent: no deposit text. */
  storageDeposit?: bigint;
}

/** The arguments of the factory's `launch`, field by field (contracts/factory/src/lib.rs). `opening_buy` is yoctoNEAR text. */
export interface LaunchArgs {
  name: string;
  ticker: string;
  icon: string | null;
  description: string | null;
  opening_buy: string | null;
}

export interface LaunchFormModel {
  counts: { name: string; ticker: string; description: string };
  errors: string[];
  /** What the launch call takes; null while any field has an error. */
  launchArgs: LaunchArgs | null;
  /** The ticker as it appears in the coin account: lowered. Null when the ticker is empty or breaks a rule. */
  accountLabel: string | null;
  buyHelp: string;
  /** `reason` is why the button is off, shown beside it; null when it is on. */
  button: { label: string; reason: string | null };
  /** The NEAR the launch attaches, shown beside the button. Null when no deposit is given. */
  deposit: { total: string; note: string } | null;
  preview: { name: string; ticker: string; description: string; openingBuy: string | null };
  /** The fee note under the button. */
  feeNote: string;
}

function launchButton(input: LaunchFormInput, label: string, valid: boolean): { label: string; reason: string | null } {
  if (input.pending) return { label: "Waiting for the wallet", reason: "Approve or close the transaction in your wallet." };
  if (!input.signedIn) return { label, reason: "Connect a wallet to launch." };
  if (!valid) return { label, reason: "Fix the items above first." };
  return { label, reason: null };
}

function depositText(storage: bigint, openingBuy: bigint | null, buyTyped: boolean): { total: string; note: string } {
  const note = "The deposit stays in the coin account. Nobody can take it out.";
  const deposit = exactNear(storage.toString());
  if (openingBuy !== null) {
    return { total: `You attach ${exactNear((storage + openingBuy).toString())} NEAR: a ${deposit} NEAR deposit and a ${exactNear(openingBuy.toString())} NEAR opening buy.`, note };
  }
  return { total: buyTyped ? `You attach ${deposit} NEAR for the deposit, plus the opening buy.` : `You attach ${deposit} NEAR: the deposit.`, note };
}

export function launchFormModel(input: LaunchFormInput): LaunchFormModel {
  const name = input.name.trim();
  const ticker = input.ticker.trim();
  const description = input.description;
  const buy = input.openingBuy.trim();
  const errors: string[] = [];
  if (name === "") errors.push("Type a name.");
  if (name.length > LAUNCH_LIMITS.name) errors.push(`The name can be ${LAUNCH_LIMITS.name} characters at most.`);
  if (ticker === "") errors.push("Type a ticker.");
  else if (!TICKER_PATTERN.test(ticker)) errors.push("The ticker can use letters, digits, - and _ only.");
  if (ticker.length > LAUNCH_LIMITS.ticker) errors.push(`The ticker can be ${LAUNCH_LIMITS.ticker} characters at most.`);
  if (description.length > LAUNCH_LIMITS.description) errors.push(`The description can be ${LAUNCH_LIMITS.description} characters at most.`);
  if (buy !== "" && parseTokenAmount(buy, NEAR_DECIMALS) === null) {
    errors.push("Type the opening buy as a number of NEAR, for example 4.40, or leave it empty.");
  }
  const icon = input.icon ?? null;
  if (icon !== null && !iconFits(icon)) errors.push("The picture must be a WebP image under 8 KB. Choose it again.");
  if (input.picture?.state === "pending") errors.push("The picture is still being shrunk. Wait for it.");
  if (input.picture?.state === "refused") errors.push(input.picture.reason);
  const openingBuyUnits = buy === "" ? null : parseTokenAmount(buy, NEAR_DECIMALS);
  const shownTicker = ticker.toUpperCase();
  const accountLabel = ticker !== "" && TICKER_PATTERN.test(ticker) && ticker.length <= LAUNCH_LIMITS.ticker ? ticker.toLowerCase() : null;
  return {
    counts: {
      name: `${input.name.length}/${LAUNCH_LIMITS.name}`,
      ticker: `${input.ticker.length}/${LAUNCH_LIMITS.ticker}`,
      description: `${description.length}/${LAUNCH_LIMITS.description}`,
    },
    errors,
    launchArgs:
      errors.length > 0
        ? null
        : { name, ticker, icon, description: description === "" ? null : description, opening_buy: openingBuyUnits === null ? null : openingBuyUnits.toString() },
    accountLabel,
    buyHelp:
      "Optional. An opening buy counts as early supply and pays the early buy fee. The creator group, sub-accounts included, can hold at most 4% of supply.",
    button: launchButton(input, `Launch ${name === "" ? "coin" : name}`, errors.length === 0),
    deposit: input.storageDeposit === undefined ? null : depositText(input.storageDeposit, openingBuyUnits, buy !== ""),
    preview: { name, ticker: shownTicker, description, openingBuy: buy === "" ? null : `${buy} NEAR` },
    feeNote: "You earn a share of the swap fee on every trade. You can claim it after the pool holds the minimum real NEAR for 24 h.",
  };
}
