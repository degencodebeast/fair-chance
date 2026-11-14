// The picture of a coin: its icon when it fits the coin's own rule, else a tile with two characters
// of the ticker on a color picked from the coin account. Everything that shows a coin picture uses this.
import { el } from "./dom-elements.ts";
import { iconFits } from "./icon-to-webp.ts";

export interface TileColors {
  background: string;
  color: string;
}

/** Dark backgrounds with white text. Every pair keeps a contrast of at least 4.5 to 1 (checked in a test). */
export const TILE_PALETTE: readonly TileColors[] = [
  { background: "#B3261E", color: "#FFFFFF" },
  { background: "#1F6B3F", color: "#FFFFFF" },
  { background: "#1D4ED8", color: "#FFFFFF" },
  { background: "#7E3AA0", color: "#FFFFFF" },
  { background: "#8A4A05", color: "#FFFFFF" },
  { background: "#0E6575", color: "#FFFFFF" },
  { background: "#A02A66", color: "#FFFFFF" },
  { background: "#475467", color: "#FFFFFF" },
];

/** FNV-1a, 32 bits. */
function fnv1a(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h;
}

export function tileColors(account: string): TileColors {
  return TILE_PALETTE[fnv1a(account) % TILE_PALETTE.length]!;
}

/** The first character, plus the last when the ticker ends in a digit, else the first two. */
export function tileText(ticker: string): string {
  const chars = [...ticker.toUpperCase()];
  if (chars.length === 0) return "?";
  if (chars.length === 1) return chars[0]!;
  const last = chars[chars.length - 1]!;
  return /^[0-9]$/.test(last) ? `${chars[0]}${last}` : `${chars[0]}${chars[1]}`;
}

function luminance(hex: string): number {
  const channel = (i: number): number => {
    const v = parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16) / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2);
}

/** The WCAG contrast ratio of two #RRGGBB colors. */
export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

export interface CoinPicture {
  account: string;
  ticker: string;
  icon?: string | null | undefined;
  small?: boolean;
}

export function drawCoinTile({ account, ticker, icon, small = false }: CoinPicture): HTMLElement {
  const size = small ? " coin-icon-small" : "";
  if (typeof icon === "string" && iconFits(icon)) {
    return el("img", { class: `coin-icon${size}`, src: icon, alt: "", width: small ? "52" : "60", height: small ? "52" : "60" });
  }
  const { background, color } = tileColors(account);
  return el("div", { class: `coin-icon coin-tile${size}`, style: `background:${background};color:${color}`, "aria-hidden": "true" }, tileText(ticker));
}
