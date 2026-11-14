// The words of the "How to try it" box. The wallet name and the faucet link live here, so one edit
// changes every place that shows them. The faucet is the testnet faucet the NEAR docs link to
// (docs.near.org, "Token Faucet").
export const WALLET_NAME = "a NEAR testnet wallet";
export const FAUCET_URL = "https://near-faucet.io/";
export const TRY_IT_TITLE = "How to try it";
export const TRY_IT_NOTE = "This runs on NEAR testnet. Test tokens have no value. The contracts have no audit.";
export const FAUCET_LINK_TEXT = "NEAR testnet faucet";

export function tryItSteps(): [string, string, string] {
  return [
    `Get ${WALLET_NAME}.`,
    "Get test NEAR from the official NEAR testnet faucet.",
    "Launch a coin, or buy one that is already here.",
  ];
}
