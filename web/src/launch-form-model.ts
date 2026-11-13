// The text and checks of the launch form. Pure. The launch button is turned off in this version:
// launching needs a wallet, and nothing on this page signs.
import { parseTokenAmount } from "./token-amount.ts";

/**
 * Field lengths of the form, from the v5 design. The coin and the factory set no limit of their
 * own (QUESTIONS.md, launch form); until they do, this is the one place the lengths live.
 */
export const LAUNCH_LIMITS = { name: 32, ticker: 8, description: 80 } as const;

const NEAR_DECIMALS = 24;

export interface LaunchFormInput {
  name: string;
  ticker: string;
  description: string;
  openingBuy: string;
}

export interface LaunchFormModel {
  counts: { name: string; ticker: string; description: string };
  errors: string[];
  buyHelp: string;
  button: { label: string; reason: string };
  preview: { name: string; ticker: string; description: string; openingBuy: string | null };
  /** The fee note under the button. */
  feeNote: string;
}

const BUTTON_REASON = "Launching is turned off in this version. A launch needs a wallet, and this page cannot connect one.";

export function launchFormModel(input: LaunchFormInput): LaunchFormModel {
  const name = input.name.trim();
  const ticker = input.ticker.trim();
  const description = input.description;
  const buy = input.openingBuy.trim();
  const errors: string[] = [];
  if (name === "") errors.push("Type a name.");
  if (name.length > LAUNCH_LIMITS.name) errors.push(`The name can be ${LAUNCH_LIMITS.name} characters at most.`);
  if (ticker === "") errors.push("Type a ticker.");
  else if (!/^[A-Za-z0-9]+$/.test(ticker)) errors.push("The ticker can use letters and digits only.");
  if (ticker.length > LAUNCH_LIMITS.ticker) errors.push(`The ticker can be ${LAUNCH_LIMITS.ticker} characters at most.`);
  if (description.length > LAUNCH_LIMITS.description) errors.push(`The description can be ${LAUNCH_LIMITS.description} characters at most.`);
  if (buy !== "" && parseTokenAmount(buy, NEAR_DECIMALS) === null) {
    errors.push("Type the opening buy as a number of NEAR, for example 4.40, or leave it empty.");
  }
  const shownTicker = ticker.toUpperCase();
  return {
    counts: {
      name: `${input.name.length}/${LAUNCH_LIMITS.name}`,
      ticker: `${input.ticker.length}/${LAUNCH_LIMITS.ticker}`,
      description: `${description.length}/${LAUNCH_LIMITS.description}`,
    },
    errors,
    buyHelp:
      "Optional. An opening buy counts as early supply and pays the early buy fee. The creator group, sub-accounts included, can hold at most 4% of supply.",
    button: { label: `Launch ${name === "" ? "coin" : name}`, reason: BUTTON_REASON },
    preview: { name, ticker: shownTicker, description, openingBuy: buy === "" ? null : `${buy} NEAR` },
    feeNote: "You earn a share of the swap fee on every trade. You can claim it after the pool holds the minimum real NEAR for 24 h.",
  };
}
