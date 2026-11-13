// The alerts screen as data. Every message is an example: the texts come from
// testdata/watcher-sample-messages/messages.json, which Go tests keep equal to what the watcher
// templates write. The page writes no alert sentence of its own.
import sample from "../../testdata/watcher-sample-messages/messages.json";
import { screenLink } from "./screen-route.ts";

export interface ChatMessage {
  from: "you" | "bot";
  head: string | null;
  text: string;
  time: string;
  /** Sample receipt ID an alert links to; null for a reply that has no receipt. */
  receipt: string | null;
  /** Link to the coin page with the receipt marked; null unless the page link names a real coin. */
  openHref: string | null;
}

export interface WatcherScreenModel {
  sampleLabel: string;
  preBuyLabel: string;
  chat: ChatMessage[];
  plain: { label: string; text: string }[];
  checkCard: string;
  /** Why the Open on Fair Chance links are off; null when they work. */
  openNote: string | null;
}

export function watcherScreenModel(href: string): WatcherScreenModel {
  const coin = new URL(href).searchParams.get("coin");
  const link = (receipt: string) => (coin ? screenLink(href, "coin", { coin, receipt }) : null);
  const chat: ChatMessage[] = [
    { from: "you", head: null, text: sample.watch_command, time: sample.watch_time, receipt: null, openHref: null },
    { from: "bot", head: null, text: sample.watch_reply, time: sample.watch_time, receipt: null, openHref: null },
    ...sample.alerts.map((a) => ({ from: "bot" as const, head: a.label, text: a.text, time: a.time, receipt: a.receipt, openHref: link(a.receipt) })),
    { from: "you", head: null, text: sample.check_command, time: sample.check_time, receipt: null, openHref: null },
    { from: "bot", head: null, text: sample.check_reply, time: sample.check_time, receipt: null, openHref: null },
  ];
  return {
    sampleLabel: "Sample data. These messages use made-up coins, accounts and times.",
    preBuyLabel: "Sample data. The same facts as /check, for a made-up coin.",
    chat,
    plain: [
      { label: "Reply to /watch", text: sample.watch_reply },
      ...sample.alerts.map((a) => ({ label: `Alert: ${a.label.toLowerCase()}`, text: a.text })),
      { label: "Reply to /check", text: sample.check_reply },
    ],
    checkCard: sample.check_reply,
    openNote: coin ? null : "Open on Fair Chance needs a real coin. Add &coin=<coin account> to this page link.",
  };
}
