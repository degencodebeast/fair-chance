// The transactions the page asks the wallet to sign: buy, sell, launch and the creator fee claim.
// Pure: no wallet, no key, no network. Each minimum comes from the coin's own quote less one fixed
// margin, SLIPPAGE_MARGIN_BPS. Gas values are the ones the recorded testnet runs used; unused gas is refunded.
import type { SellQuote } from "./account-views.ts";
import type { BuyQuote } from "./buy-quote.ts";
import type { ChainReader } from "./chain.ts";
import { parseStorageMin } from "./coin-views.ts";
import { launchFormModel, type LaunchFormInput } from "./launch-form-model.ts";
import { refusalText } from "./refusal-texts.ts";
import { parseTokenAmount } from "./token-amount.ts";
import type { FunctionCallAction, TxRequest } from "./tx-types.ts";

const TGAS = 1_000_000_000_000n;
/** Gas of the buy action. The testnet runs sent storage_deposit and buy with 50 TGas each. */
export const BUY_GAS = 50n * TGAS;
/** Gas of the storage_deposit action sent before a first buy. */
export const STORAGE_DEPOSIT_GAS = 50n * TGAS;

/** The price move a buy or a sell accepts after its quote: 100 bps, 1%. The builder's choice; callers do not set it. */
export const SLIPPAGE_MARGIN_BPS = 100n;
const BPS = 10_000n;

/** Whether the buyer has a NEP-145 storage record on the coin, and what one costs if not. */
export type BuyerStorage = { registered: true } | { registered: false; storageMin: bigint };

function call(methodName: string, args: Record<string, unknown>, gas: bigint, deposit: bigint): FunctionCallAction {
  return { type: "FunctionCall", params: { methodName, args, gas: gas.toString(), deposit: deposit.toString() } };
}

/**
 * Reads whether `accountId` has a storage record on the coin (`storage_balance_of` is null when
 * not) and, if not, the coin's `storage_balance_bounds.min`, the exact deposit one record needs.
 */
export async function readBuyerStorage(chain: Pick<ChainReader, "view">, accountId: string): Promise<BuyerStorage> {
  const record = await chain.view<unknown>("storage_balance_of", { account_id: accountId });
  if (record !== null) return { registered: true };
  return { registered: false, storageMin: parseStorageMin(await chain.view("storage_balance_bounds")) };
}

/**
 * One buy of `nearIn` yoctoNEAR. `min_tokens_out` is the quote's `tokens_out` less the margin,
 * rounded down, so the coin refuses with FC_SLIPPAGE if the price moved more than 1% after the quote.
 * A buyer with no storage record gets `storage_deposit` first, in the same transaction: the coin
 * refuses a buy from an unregistered account, and one failed action rolls back both.
 */
export function buildBuy(coin: string, input: { nearIn: bigint; quote: BuyQuote; storage: BuyerStorage }): TxRequest[] {
  const minTokensOut = (input.quote.tokensOut * (BPS - SLIPPAGE_MARGIN_BPS)) / BPS;
  const actions = input.storage.registered ? [] : [call("storage_deposit", {}, STORAGE_DEPOSIT_GAS, input.storage.storageMin)];
  actions.push(call("buy", { min_tokens_out: minTokensOut.toString() }, BUY_GAS, input.nearIn));
  return [{ receiverId: coin, actions }];
}

/** Gas of a sell: the testnet runs sent each sell with 100 TGas (the coin pays the NEAR out in a transfer). */
export const SELL_GAS = 100n * TGAS;
/** Gas of a creator fee claim: the transfer plus the 10 TGas on_creator_claim callback. The sandbox claim test sent 300 TGas. */
export const CLAIM_GAS = 300n * TGAS;
/** The deposit a sell and a claim need: exactly 1 yoctoNEAR, else FC_ONE_YOCTO. */
const ONE_YOCTO = 1n;

export type SellTx = { kind: "ready"; txs: TxRequest[] } | { kind: "nothing-fills"; text: string };

const NOTHING_FILLS = "No tokens can sell now, so there is no sell to send.";

/**
 * One sell of `amount` token base units. The coin reads `min_near_out` as the NEAR accepted for
 * all of `amount` and checks it per token of the part that fills (rule-engine actions.rs: refused
 * when near_out x amount < min_near_out x tokens_sold). So `min_near_out` is the quoted price per
 * filled token, scaled to all of `amount`, less the margin: near_out x amount x 9,900 ÷
 * (tokens_filled x 10,000), multiplied before the one division, rounded down. A quote that fills
 * no tokens gives no sell.
 */
export function buildSell(coin: string, input: { amount: bigint; quote: SellQuote }): SellTx {
  const { nearOut, tokensFilled } = input.quote;
  if (tokensFilled === 0n) return { kind: "nothing-fills", text: NOTHING_FILLS };
  const minNearOut = (nearOut * input.amount * (BPS - SLIPPAGE_MARGIN_BPS)) / (tokensFilled * BPS);
  const args = { amount: input.amount.toString(), min_near_out: minNearOut.toString() };
  return { kind: "ready", txs: [{ receiverId: coin, actions: [call("sell", args, SELL_GAS, ONE_YOCTO)] }] };
}

/** The creator claims its whole fee balance. The coin refuses any other caller with FC_NOT_CREATOR. */
export function buildClaimCreatorFees(coin: string): TxRequest[] {
  return [{ receiverId: coin, actions: [call("claim_creator_fees", {}, CLAIM_GAS, ONE_YOCTO)] }];
}

/**
 * The NEAR a launch attaches for the coin's storage, on top of any opening buy. The coin's `new`
 * refuses a launch whose deposit does not cover its storage plus 8 bytes of later state growth
 * (the factory then refunds it). On the sandbox, a launch with an 8191-byte icon locked 0.088940 NEAR
 * (8894 bytes) and went through at 0.25 NEAR and at 0.20 NEAR; the form's text limits add at most
 * about 250 bytes. 0.25 NEAR leaves about 0.16 NEAR of margin. The NEAR above the storage lock
 * stays in the coin account.
 */
export const LAUNCH_STORAGE_DEPOSIT = 250_000_000_000_000_000_000_000n;
/** Gas of the factory launch: create, fund, `new` (50 TGas) and two callbacks. The testnet launch sent 300 TGas. */
export const LAUNCH_GAS = 300n * TGAS;
const NEAR_DECIMALS = 24;

/** The coin's icon rule (rule-engine coin_state.rs): a WebP data URL of at most 8 * 1024 - 1 bytes, prefix included. */
const ICON_PREFIX = "data:image/webp;base64,";
export const ICON_MAX_BYTES = 8 * 1024 - 1;
const ICON_REFUSAL = "icon must be a WebP data URL (data:image/webp;base64,...) under 8192 bytes";

function iconIsSmallWebp(icon: string): boolean {
  if (!icon.startsWith(ICON_PREFIX)) return false;
  const base64 = icon.slice(ICON_PREFIX.length);
  return new TextEncoder().encode(icon).length <= ICON_MAX_BYTES && /^[A-Za-z0-9+/=]+$/.test(base64);
}

export type LaunchTx =
  | { kind: "ready"; txs: TxRequest[]; /** The account the factory makes: `<ticker lowered>.<factory>`. */ coin: string; deposit: bigint }
  | { kind: "invalid"; errors: string[] };

/**
 * The factory `launch` call from the launch form. The ticker goes as typed in upper case (the coin's
 * symbol, as the form previews it); the factory lowers it for the coin account. An empty
 * description and an empty opening buy go as null. Deposit: `storageDeposit` (default
 * LAUNCH_STORAGE_DEPOSIT) plus the opening buy.
 */
export function buildLaunch(factory: string, input: { form: LaunchFormInput; icon: string | null; storageDeposit?: bigint }): LaunchTx {
  const errors = [...launchFormModel(input.form).errors];
  if (input.icon !== null && !iconIsSmallWebp(input.icon)) errors.push(refusalText(ICON_REFUSAL)!);
  if (errors.length > 0) return { kind: "invalid", errors };
  const ticker = input.form.ticker.trim();
  const description = input.form.description.trim();
  const buyText = input.form.openingBuy.trim();
  const openingBuy = buyText === "" ? null : parseTokenAmount(buyText, NEAR_DECIMALS);
  const deposit = (input.storageDeposit ?? LAUNCH_STORAGE_DEPOSIT) + (openingBuy ?? 0n);
  const args = {
    name: input.form.name.trim(),
    ticker: ticker.toUpperCase(),
    icon: input.icon,
    description: description === "" ? null : description,
    opening_buy: openingBuy === null ? null : openingBuy.toString(),
  };
  return { kind: "ready", coin: `${ticker.toLowerCase()}.${factory}`, deposit, txs: [{ receiverId: factory, actions: [call("launch", args, LAUNCH_GAS, deposit)] }] };
}
