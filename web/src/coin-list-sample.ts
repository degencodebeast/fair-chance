// Made-up coins for the coin list, shown with a "Sample data" label on each card when the page has
// no factory to read. The accounts end in ".sample", so none of them can be opened as a coin.
import { linePoints, type PricePoint } from "./price-series.ts";
import type { CoinCard } from "./coin-list-model.ts";

function line(values: number[]): string {
  const points: PricePoint[] = values.map((v, i) => ({ block: i, timeNs: null, nearPerMillion: v }));
  return linePoints(points, 200, 40, 3);
}

export const SAMPLE_COINS: CoinCard[] = [
  { account: "moth.sample", name: "Moth Lamp", ticker: "MOTH", age: "1 h 22 min", ageSeconds: 4920, marketCapNear: 170.41, marketCap: "170.41 NEAR", change: "+70.4%", early: "18.4%", cap: { label: "Cap full until 15:02", tone: "full" }, latest: { account: "bigfish.sample", text: "tried to sell 30.0M MOTH. The rule filled 7.1M. It can sell up to 7.1M more after 15:02." }, bigSellHeld: true, inEarlyWindow: false, lastSellAtNs: 1794582060000000000n, line: line([10, 14, 13, 19, 24, 22, 30, 34, 31, 36, 33, 35]), source: "sample" },
  { account: "sock.sample", name: "Wet Sock", ticker: "SOCK", age: "6 s", ageSeconds: 6, marketCapNear: 42.1, marketCap: "42.10 NEAR", change: "+5.2%", early: "6.2% so far", cap: { label: "Early window, 11 blocks left", tone: "early" }, latest: { account: "dart.sample", text: "bought 2.0M SOCK in block 3. These tokens count as early supply." }, bigSellHeld: false, inEarlyWindow: true, lastSellAtNs: null, line: line([20, 21, 23, 24]), source: "sample" },
  { account: "glue.sample", name: "Glue Cat", ticker: "GLUE", age: "9 s", ageSeconds: 9, marketCapNear: 38.75, marketCap: "38.75 NEAR", change: "-3.1%", early: "9.8% so far", cap: { label: "Early window, 3 blocks left", tone: "early" }, latest: { account: "bigfish.sample", text: "bought 6.1M GLUE in block 2. These tokens count as early supply." }, bigSellHeld: false, inEarlyWindow: true, lastSellAtNs: null, line: line([24, 26, 23, 22]), source: "sample" },
  { account: "pgn.sample", name: "Pigeon Court", ticker: "PGN", age: "9 min", ageSeconds: 540, marketCapNear: 88.3, marketCap: "88.30 NEAR", change: "+12.6%", early: "21.0%", cap: { label: "Cap open, 0.12 of 1.10 NEAR out", tone: "open" }, latest: { account: "", text: "No sells yet." }, bigSellHeld: false, inEarlyWindow: false, lastSellAtNs: null, line: line([18, 20, 19, 23, 25, 24, 27]), source: "sample" },
  { account: "soup.sample", name: "Soup Clock", ticker: "SOUP", age: "40 min", ageSeconds: 2400, marketCapNear: 131.06, marketCap: "131.06 NEAR", change: "-8.4%", early: "15.7%", cap: { label: "Cap full until 14:44", tone: "full" }, latest: { account: "dart.sample", text: "tried to sell 12.0M SOUP. The rule filled 3.0M. It can sell up to 3.0M more after 14:44." }, bigSellHeld: true, inEarlyWindow: false, lastSellAtNs: 1794580980000000000n, line: line([30, 34, 36, 31, 28, 29, 25, 27, 26]), source: "sample" },
  { account: "ktlm.sample", name: "Kettle Moon", ticker: "KTLM", age: "3 h 5 min", ageSeconds: 11100, marketCapNear: 260.4, marketCap: "260.40 NEAR", change: "+2.9%", early: "12.3%", cap: { label: "Cap open, 0.40 of 3.02 NEAR out", tone: "open" }, latest: { account: "dart.sample", text: "sold 1.4M KTLM. It filled in full." }, bigSellHeld: false, inEarlyWindow: false, lastSellAtNs: 1794577000000000000n, line: line([22, 25, 24, 28, 27, 26, 29, 31, 28, 30, 29, 30]), source: "sample" },
];
