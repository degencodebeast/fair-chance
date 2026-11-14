// The words of the "How to try it" box. The wallet name and the faucet link live here, so one edit
// changes every place that shows them. The faucet is the NEAR docs page "Token Faucet", which holds the built-in testnet
// faucet and lists the external one.
export const WALLET_NAME = "Meteor Wallet";
export const FAUCET_URL = "https://docs.near.org/getting-started/faucet";
export const TRY_IT_TITLE = "How to try it";
export const TRY_IT_NOTE = "This runs on NEAR testnet. Test tokens have no value. The contracts have no audit.";
export const FAUCET_LINK_TEXT = "NEAR testnet faucet";

export function tryItSteps(): [string, string, string] {
  return [
    `Install ${WALLET_NAME} and switch it to testnet.`,
    "Get test NEAR from the official NEAR testnet faucet.",
    "Launch a coin, or buy one that is already here.",
  ];
}
