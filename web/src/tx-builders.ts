// The transactions the page asks the wallet to sign: buy, sell, launch and the creator fee claim.
// Pure: no wallet, no key, no network. Each minimum comes from the coin's own quote, with no margin
// added here. Gas values are the ones the recorded testnet runs used; unused gas is refunded.
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
 * One buy of `nearIn` yoctoNEAR. `min_tokens_out` is the quote's `tokens_out` (INTERFACE
 * fc_quote_buy), so the coin refuses with FC_SLIPPAGE if the price moved after the quote.
 * A buyer with no storage record gets `storage_deposit` first, in the same transaction: the coin
 * refuses a buy from an unregistered account, and one failed action rolls back both.
 */
export function buildBuy(coin: string, input: { nearIn: bigint; quote: BuyQuote; storage: BuyerStorage }): TxRequest[] {
  const actions = input.storage.registered ? [] : [call("storage_deposit", {}, STORAGE_DEPOSIT_GAS, input.storage.storageMin)];
  actions.push(call("buy", { min_tokens_out: input.quote.tokensOut.toString() }, BUY_GAS, input.nearIn));
  return [{ receiverId: coin, actions }];
}

/** Gas of a sell: the testnet runs sent each sell with 100 TGas (the coin pays the NEAR out in a transfer). */
export const SELL_GAS = 100n * TGAS;
/** Gas of a creator fee claim: the transfer plus the 10 TGas on_creator_claim callback. The sandbox claim test sent 300 TGas. */
export const CLAIM_GAS = 300n * TGAS;
/** The deposit a sell and a claim need: exactly 1 yoctoNEAR, else FC_ONE_YOCTO. */
const ONE_YOCTO = 1n;

/**
 * One sell of `amount` token base units. `min_near_out` is the quote's `near_out` for that amount
 * (INTERFACE sell: the NEAR accepted for all of `amount`, checked per token of the part that fills).
 */
export function buildSell(coin: string, input: { amount: bigint; quote: SellQuote }): TxRequest[] {
  const args = { amount: input.amount.toString(), min_near_out: input.quote.nearOut.toString() };
  return [{ receiverId: coin, actions: [call("sell", args, SELL_GAS, ONE_YOCTO)] }];
}

/** The creator claims its whole fee balance. The coin refuses any other caller with FC_NOT_CREATOR. */
export function buildClaimCreatorFees(coin: string): TxRequest[] {
  return [{ receiverId: coin, actions: [call("claim_creator_fees", {}, CLAIM_GAS, ONE_YOCTO)] }];
}

/**
 * The NEAR a launch attaches for the coin's storage, on top of any opening buy. The coin's `new`
 * refuses a launch whose deposit does not cover its storage plus 8 bytes of later state growth
 * (the factory then refunds it). The testnet CABAL2 launch attached 1 NEAR with no icon and locked
 * 0.0402 NEAR for storage; an 8191-byte icon adds about 0.082 NEAR (10^19 yoctoNEAR per byte).
 * The NEAR above the storage lock stays in the coin account.
 */
export const LAUNCH_STORAGE_DEPOSIT = 10n ** 24n;
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
 * description and an empty opening buy go as null. Deposit: LAUNCH_STORAGE_DEPOSIT plus the opening buy.
 */
export function buildLaunch(factory: string, input: { form: LaunchFormInput; icon: string | null }): LaunchTx {
  const errors = [...launchFormModel(input.form).errors];
  if (input.icon !== null && !iconIsSmallWebp(input.icon)) errors.push(refusalText(ICON_REFUSAL)!);
  if (errors.length > 0) return { kind: "invalid", errors };
  const ticker = input.form.ticker.trim();
  const description = input.form.description.trim();
  const buyText = input.form.openingBuy.trim();
  const openingBuy = buyText === "" ? null : parseTokenAmount(buyText, NEAR_DECIMALS);
  const deposit = LAUNCH_STORAGE_DEPOSIT + (openingBuy ?? 0n);
  const args = {
    name: input.form.name.trim(),
    ticker: ticker.toUpperCase(),
    icon: input.icon,
    description: description === "" ? null : description,
    opening_buy: openingBuy === null ? null : openingBuy.toString(),
  };
  return { kind: "ready", coin: `${ticker.toLowerCase()}.${factory}`, deposit, txs: [{ receiverId: factory, actions: [call("launch", args, LAUNCH_GAS, deposit)] }] };
}
