// Draws the price chart of the coin page from the swap events it already read: no extra read.
// The line is the price each trade got, so the caption says so.
import type { CoinEvent } from "./chain.ts";
import { el } from "./dom-elements.ts";
import { clockTime } from "./format.ts";
import { changeText, linePoints, priceSeries } from "./price-series.ts";

const SVG = "http://www.w3.org/2000/svg";
const WIDTH = 600;
const HEIGHT = 220;

export function drawPriceChart(events: CoinEvent[], symbol: string): HTMLElement {
  const points = priceSeries(events);
  const head = el("div", { class: "section-head" }, el("h2", {}, "Price chart"), el("span", { class: "muted" }, `Price of each trade, NEAR for 1M ${symbol}`));
  if (points.length < 2) {
    return el("section", { id: "price-chart", class: "card", "aria-label": "Price chart" }, head, el("p", { class: "note", id: "chart-empty" }, "The chart needs two trades to draw a line. This coin has not had them yet."));
  }
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", `0 0 ${WIDTH} ${HEIGHT}`);
  svg.setAttribute("preserveAspectRatio", "none");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", `Price of each trade, ${changeText(points) ?? ""} from the earliest trade to the latest`);
  svg.setAttribute("class", "chart-svg");
  const line = document.createElementNS(SVG, "polyline");
  line.setAttribute("points", linePoints(points, WIDTH, HEIGHT, 8));
  line.setAttribute("fill", "none");
  line.setAttribute("stroke", "#17150F");
  line.setAttribute("stroke-width", "2.5");
  line.setAttribute("stroke-linejoin", "round");
  line.setAttribute("vector-effect", "non-scaling-stroke");
  svg.append(line);
  const prices = points.map((p) => p.nearPerMillion);
  const first = points[0]!;
  const last = points[points.length - 1]!;
  const time = (ns: string | null, block: number) => (ns === null ? `block ${block}` : clockTime(BigInt(ns), {}));
  const price = (n: number) => n.toFixed(4);
  return el(
    "section",
    { id: "price-chart", class: "card", "aria-label": "Price chart" },
    head,
    svg,
    el(
      "div",
      { class: "chart-axis num" },
      el("span", {}, time(first.timeNs, first.block)),
      el("span", {}, `Low ${price(Math.min(...prices))}, high ${price(Math.max(...prices))} NEAR`),
      el("span", {}, time(last.timeNs, last.block)),
    ),
    el("p", { class: "chart-caption muted" }, `${points.length} trades, ${changeText(points)} from the earliest trade to the latest. A buy price includes the fee. A sell price is what the seller received.`),
  );
}
