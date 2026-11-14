import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const OLD_WORDING = /not live|not open yet|public (?:alert )?bot is not/i;

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? files(path) : [path];
  });
}

describe("the bot is on", () => {
  const src = join(import.meta.dirname, "../src");
  it("leaves no 'not live' or 'not open yet' text in the source", () => {
    for (const file of files(src)) expect(readFileSync(file, "utf8"), file).not.toMatch(OLD_WORDING);
  });
  it("leaves none in the built page either, when a build exists", () => {
    const dist = join(import.meta.dirname, "../dist");
    if (!existsSync(dist)) return;
    for (const file of files(dist).filter((f) => /\.(?:js|html|css)$/.test(f))) expect(readFileSync(file, "utf8"), file).not.toMatch(OLD_WORDING);
  });
});
