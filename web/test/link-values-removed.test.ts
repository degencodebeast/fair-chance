import { describe, expect, it } from "vitest";
import { withoutLinkValues } from "../scripts/hosted-page-check.ts";

const link = "https://host.example/?coin=moth.factory.testnet&rpc_key=SECRET%2FKEY%20123&plain=PLAINVALUE#frag_key=SECRETFRAG&other=2NDFRAG";

describe("withoutLinkValues", () => {
  it("removes the full link", () => {
    expect(withoutLinkValues(`failed at ${link}`, link)).not.toContain("SECRET");
  });

  it("removes a decoded query value", () => {
    expect(withoutLinkValues("bad PLAINVALUE here", link)).not.toContain("PLAINVALUE");
    expect(withoutLinkValues("bad SECRET/KEY 123 here", link)).not.toContain("SECRET/KEY");
  });

  it("removes the encoded form of a query value", () => {
    expect(withoutLinkValues("bad SECRET%2FKEY%20123 here", link)).not.toContain("SECRET");
    expect(withoutLinkValues("bad SECRET%2fKEY%20123 here", link)).not.toContain("SECRET");
  });

  it("removes the fragment and its values", () => {
    const out = withoutLinkValues("bad #frag_key=SECRETFRAG&other=2NDFRAG and SECRETFRAG and 2NDFRAG", link);
    expect(out).not.toContain("SECRETFRAG");
    expect(out).not.toContain("2NDFRAG");
  });
});
