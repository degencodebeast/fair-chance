// The alerts screen as data. Every message is an example: the texts come from
// testdata/watcher-sample-messages/messages.json, which Go tests keep equal to what the watcher
// templates write. The page writes no alert sentence of its own.
import sample from "../../testdata/watcher-sample-messages/messages.json";

export interface ChatMessage {
  from: "you" | "bot";
  head: string | null;
  text: string;
  time: string;
  /** Sample receipt ID an alert links to; null for a reply that has no receipt. */
  receipt: string | null;
  /** Always null today: a sample alert has no receipt on chain, so no link can mark one. */
  openHref: string | null;
}

export interface WatcherScreenModel {
  sampleLabel: string;
  chatLabel: string;
  plainLabel: string;
  preBuyLabel: string;
  chat: ChatMessage[];
  plain: { label: string; text: string }[];
  checkCard: string;
  /** Why the Open on Fair Chance links are off; null when they work. */
  openNote: string | null;
}

export function watcherScreenModel(_href: string): WatcherScreenModel {
  const chat: ChatMessage[] = [
    { from: "you", head: null, text: sample.watch_command, time: sample.watch_time, receipt: null, openHref: null },
    { from: "bot", head: null, text: sample.watch_reply, time: sample.watch_time, receipt: null, openHref: null },
    ...sample.alerts.map((a) => ({ from: "bot" as const, head: null, text: a.text, time: a.time, receipt: a.receipt, openHref: null })),
    { from: "you", head: null, text: sample.check_command, time: sample.check_time, receipt: null, openHref: null },
    { from: "bot", head: null, text: sample.check_reply, time: sample.check_time, receipt: null, openHref: null },
  ];
  return {
    sampleLabel: "Sample data. These messages use made-up coins, accounts and times.",
    chatLabel: "Sample data. A made-up chat with made-up coins, accounts and times.",
    plainLabel: "Sample data. The same made-up messages as plain text.",
    preBuyLabel: "Sample data. The same facts as /check, for a made-up coin.",
    chat,
    plain: [
      { label: "Reply to /watch", text: sample.watch_reply },
      ...sample.alerts.map((a) => ({ label: `Alert: ${a.label.toLowerCase()}`, text: a.text })),
      { label: "Reply to /check", text: sample.check_reply },
    ],
    checkCard: sample.check_reply,
    openNote: "Open on Fair Chance is turned off here: a sample alert has no real receipt on the chain, so there is nothing to open.",
  };
}
