// The wallet list the wallet library shows, kept in the repo. Without it the library loads its list
// from a GitHub branch at run time, so the wallets and their code could change after a review.
// One wallet: Meteor Wallet. It works on NEAR testnet, it can send a list of transactions, and it
// opens in a pop-up or its browser extension, so the page keeps its state. Its code loads from one
// fixed commit of the Meteor repo (the commit of 11 Nov 2026), not from the branch that moves.
// Source of the entry: the near-connect manifest, version 1.1.0, read on 14 Nov 2026.
import type { NearConnectorOptions } from "@hot-labs/near-connect";

type Manifest = Exclude<NonNullable<NearConnectorOptions["manifest"]>, string>;

const METEOR_COMMIT = "6369ad789d2f3aaf8e6ccb065021723111027174";

// A plain round picture drawn by the page, so the wallet picker fetches no picture from another host.
const ICON = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Ccircle cx='8' cy='8' r='7' fill='%23555'/%3E%3C/svg%3E";

export const WALLET_MANIFEST: Manifest = {
  version: "1.1.0",
  wallets: [
    {
      id: "meteor-wallet",
      name: "Meteor Wallet",
      icon: ICON,
      description: "A NEAR wallet in the browser and as an extension.",
      website: "https://meteorwallet.app/",
      version: "1.1.1",
      executor: `https://raw.githubusercontent.com/Meteor-Wallet/meteor_wallet_sdk/${METEOR_COMMIT}/storage/meteor-near-connect-latest.js`,
      type: "sandbox",
      platform: [],
      features: {
        signMessage: true,
        signTransaction: false,
        signInWithoutAddKey: true,
        signInAndSignMessage: true,
        signAndSendTransaction: true,
        signAndSendTransactions: true,
        signInWithFunctionCallKey: true,
        signDelegateActions: true,
        mainnet: true,
        testnet: true,
      },
      permissions: {
        storage: true,
        allowsOpen: ["https://chromewebstore.google.com", "https://wallet.meteorwallet.app", "https://meteorwallet.app", "meteorwallet://", "meteorwalletdev://"],
        external: ["meteorCom", "meteorComV2"],
      },
    },
  ],
};
