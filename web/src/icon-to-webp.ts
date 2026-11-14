// Turns any picture the visitor chooses into a WebP data URL the coin accepts. The coin refuses an
// icon that does not start with "data:image/webp;base64," or whose whole text is longer than
// 8 * 1024 - 1 characters (rule-engine/src/coin_state.rs, icon_is_small_webp). `iconFits` is the
// same check. Only `browserIconLoader` touches the browser; the rest takes an injected loader.

export const ICON_PREFIX = "data:image/webp;base64,";
export const ICON_MAX_CHARS = 8 * 1024 - 1;

/** Square sizes in pixels, then qualities, tried from the best picture to the smallest. */
const SIZES = [256, 192, 128, 96, 64] as const;
const QUALITIES = [0.8, 0.6, 0.4] as const;

/** Draws the picture at `size` x `size` px, center cropped, and returns the data URL the browser makes. */
export type IconEncoder = (size: number, quality: number) => Promise<string>;
/** Reads a picture file once and returns its encoder. Throws when the file is not a picture. */
export type IconLoader = (file: File) => Promise<IconEncoder>;

export type IconResult = { ok: true; dataUrl: string } | { ok: false; reason: string };

/** The coin's rule: the WebP prefix, a base64 body that is not empty, and at most 8191 characters in all. */
export function iconFits(dataUrl: string): boolean {
  if (!dataUrl.startsWith(ICON_PREFIX)) return false;
  const body = dataUrl.slice(ICON_PREFIX.length);
  return dataUrl.length <= ICON_MAX_CHARS && body.length > 0 && /^[A-Za-z0-9+/=]+$/.test(body);
}

const TOO_BIG = "This picture does not fit in 8 KB, even at 64 px. Choose a simpler picture.";
const NO_WEBP = "This browser cannot make WebP pictures. Use a current Chrome, Edge or Firefox.";
const NOT_A_PICTURE = "This page could not read this file as a picture. Choose a PNG, JPEG, GIF, WebP or SVG file.";

export async function iconToWebp(file: File, load: IconLoader = browserIconLoader): Promise<IconResult> {
  if (!file.type.startsWith("image/")) return { ok: false, reason: NOT_A_PICTURE };
  let encode: IconEncoder;
  try {
    encode = await load(file);
  } catch {
    return { ok: false, reason: NOT_A_PICTURE };
  }
  for (const size of SIZES) {
    for (const quality of QUALITIES) {
      let dataUrl: string;
      try {
        dataUrl = await encode(size, quality);
      } catch {
        return { ok: false, reason: NOT_A_PICTURE };
      }
      // A browser that cannot encode WebP answers with a PNG. Smaller sizes will not change that.
      if (!dataUrl.startsWith(ICON_PREFIX)) return { ok: false, reason: NO_WEBP };
      if (iconFits(dataUrl)) return { ok: true, dataUrl };
    }
  }
  return { ok: false, reason: TOO_BIG };
}

function readDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("read failed"));
    reader.readAsDataURL(blob);
  });
}

/** The real browser parts: decode through an image element (it reads SVG too) and draw on a canvas. */
export const browserIconLoader: IconLoader = async (file) => {
  const url = URL.createObjectURL(file);
  const image = new Image();
  try {
    image.src = url;
    await image.decode();
  } finally {
    URL.revokeObjectURL(url);
  }
  // An SVG with no size of its own decodes to 0 x 0 in some browsers; draw it at the largest size then.
  const width = image.naturalWidth || 256;
  const height = image.naturalHeight || 256;
  const side = Math.min(width, height);
  const sx = (width - side) / 2;
  const sy = (height - side) / 2;
  return async (size, quality) => {
    const canvas = document.createElement("canvas");
    canvas.width = size;
    canvas.height = size;
    const context = canvas.getContext("2d");
    if (context === null) throw new Error("no canvas");
    context.drawImage(image, sx, sy, side, side, 0, 0, size, size);
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/webp", quality));
    if (blob === null) return "";
    return readDataUrl(blob);
  };
};
