// The page's one wallet control, and the wallet bar every screen shows. The wallet library is a
// separate script file: it loads only when the visitor connects, or when the page takes up a
// session saved by an earlier visit, and only on a testnet page (wallet-session.ts checks that).
import { walletBar } from "./screen-chrome.ts";
import { walletControl, type WalletAccount, type WalletControl, type WalletLibrary } from "./wallet-session.ts";

/** A wallet library that loads on its first call. Events listened to before then start once it loads. */
export function lazyLibrary(load: () => Promise<WalletLibrary>): WalletLibrary {
  let loaded: Promise<WalletLibrary> | null = null;
  const signIns: ((account: WalletAccount) => void)[] = [];
  const signOuts: (() => void)[] = [];
  const lib = (): Promise<WalletLibrary> => {
    if (loaded === null) {
      loaded = load().then((real) => {
        real.onSignIn((account) => signIns.forEach((l) => l(account)));
        real.onSignOut(() => signOuts.forEach((l) => l()));
        return real;
      });
      // A failed load is tried again on the next call.
      loaded.catch(() => (loaded = null));
    }
    return loaded;
  };
  return {
    connect: async () => (await lib()).connect(),
    restore: async () => (await lib()).restore(),
    send: async (account, txs) => (await lib()).send(account, txs),
    signOut: async (account) => (await lib()).signOut(account),
    onSignIn: (listener) => void signIns.push(listener),
    onSignOut: (listener) => void signOuts.push(listener),
  };
}

/** Puts the wallet bar before the app, once. A redraw of a screen does not remove it. */
export function mountWalletBar(doc: Document, control: WalletControl): void {
  const app = doc.querySelector("#app");
  if (app === null || doc.querySelector("#wallet-bar") !== null) return;
  app.before(walletBar(control));
}

let shared: WalletControl | null = null;

/** The one wallet control of this page. The first call takes up a saved session. */
export function pageWallet(href: string = location.href): WalletControl {
  if (shared === null) {
    shared = walletControl(href, () => lazyLibrary(async () => (await import("./wallet-adapter.ts")).nearConnectLibrary()));
    void shared.restore();
  }
  return shared;
}
