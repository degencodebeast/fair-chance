import { describe, expect, it } from "vitest";
import { ICON_MAX_CHARS, ICON_PREFIX, iconFits, iconToWebp, type IconEncoder } from "../src/icon-to-webp.ts";
import { copyViolations } from "./copy-rules.ts";

const webp = (length: number) => ICON_PREFIX + "A".repeat(length - ICON_PREFIX.length);
const file = new File(["x"], "pic.png", { type: "image/png" });

/** An encoder whose output length depends on the size and the quality asked for. */
function encoderBySize(lengthFor: (size: number, quality: number) => number, calls: Array<[number, number]> = []): IconEncoder {
  return async (size, quality) => {
    calls.push([size, quality]);
    return webp(lengthFor(size, quality));
  };
}
const loader = (encoder: IconEncoder) => async () => encoder;

describe("iconFits", () => {
  it("takes a WebP data URL of 8191 characters and refuses one of 8192", () => {
    expect(ICON_MAX_CHARS).toBe(8191);
    expect(iconFits(webp(8191))).toBe(true);
    expect(iconFits(webp(8192))).toBe(false);
  });

  it("refuses a wrong prefix, an empty body and a character outside base64", () => {
    expect(iconFits("data:image/png;base64,AAAA")).toBe(false);
    expect(iconFits("data:image/webp;base64,")).toBe(false);
    expect(iconFits("data:image/webp;base64,AA AA")).toBe(false);
    expect(iconFits("data:image/webp;base64,AA-A")).toBe(false);
    expect(iconFits("AAAA")).toBe(false);
  });

  it("takes the base64 characters + / and =", () => {
    expect(iconFits(`${ICON_PREFIX}Az09+/==`)).toBe(true);
  });
});

describe("iconToWebp", () => {
  it("returns the first picture that fits, trying 256 px at quality 0.8 first", async () => {
    const calls: Array<[number, number]> = [];
    const result = await iconToWebp(file, loader(encoderBySize(() => 500, calls)));
    expect(result).toEqual({ ok: true, dataUrl: webp(500) });
    expect(calls).toEqual([[256, 0.8]]);
  });

  it("lowers the quality, then the size, until the picture fits", async () => {
    const calls: Array<[number, number]> = [];
    // Fits only at 128 px or less, at any quality.
    const result = await iconToWebp(file, loader(encoderBySize((size) => (size <= 128 ? 4000 : 9000), calls)));
    expect(result).toEqual({ ok: true, dataUrl: webp(4000) });
    const sizes = [...new Set(calls.map(([s]) => s))];
    expect(sizes).toEqual([256, 192, 128]);
    expect(calls.every(([, q]) => q >= 0.4 && q <= 0.8)).toBe(true);
    expect(calls[0]).toEqual([256, 0.8]);
  });

  it("refuses with a plain sentence when the smallest try is still too large", async () => {
    const calls: Array<[number, number]> = [];
    const result = await iconToWebp(file, loader(encoderBySize(() => 9000, calls)));
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toMatch(/8 KB/);
      expect(copyViolations(result.reason)).toEqual([]);
    }
    expect(calls.at(-1)).toEqual([64, 0.4]);
  });

  it("refuses when the browser gives a PNG for the WebP request", async () => {
    const encoder: IconEncoder = async () => "data:image/png;base64,AAAA";
    const result = await iconToWebp(file, loader(encoder));
    expect(result).toEqual({ ok: false, reason: expect.stringMatching(/cannot make WebP/) });
  });

  it("stops at the first PNG answer instead of trying every size", async () => {
    let n = 0;
    const encoder: IconEncoder = async () => {
      n++;
      return "data:image/png;base64,AAAA";
    };
    await iconToWebp(file, loader(encoder));
    expect(n).toBe(1);
  });

  it("refuses with a plain sentence when the file cannot be read as a picture", async () => {
    const result = await iconToWebp(file, async () => {
      throw new Error("decode failed");
    });
    expect(result).toEqual({ ok: false, reason: expect.stringMatching(/not read this file as a picture/) });
  });

  it("refuses a file that is not an image without loading it", async () => {
    let loaded = false;
    const result = await iconToWebp(new File(["x"], "a.txt", { type: "text/plain" }), async () => {
      loaded = true;
      return encoderBySize(() => 10);
    });
    expect(loaded).toBe(false);
    expect(result.ok).toBe(false);
  });
});
