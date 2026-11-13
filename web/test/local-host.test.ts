import { describe, expect, it } from "vitest";
import { isOwnMachineHost, isSandboxRpcHost } from "../src/local-host.ts";

describe("which hosts count as the visitor's own machine", () => {
  it.each(["localhost", "app.localhost", "[::1]", "0.0.0.0", "127.0.0.1", "127.0.0.2", "127.255.255.254"])("%s skips the hosted redirect", (host) => {
    expect(isOwnMachineHost(host)).toBe(true);
  });
  it.each(["localhost.evil.example", "127.0.0.1.evil.example", "128.0.0.1", "notlocalhost", "example.com"])("%s does not skip it", (host) => {
    expect(isOwnMachineHost(host)).toBe(false);
  });
});

describe("which hosts a link may give as the sandbox RPC", () => {
  it.each(["localhost", "127.0.0.1"])("%s is the sandbox host", (host) => {
    expect(isSandboxRpcHost(host)).toBe(true);
  });
  it.each(["[::1]", "0.0.0.0", "127.0.0.2", "app.localhost", "localhost.evil.example"])("%s is refused", (host) => {
    expect(isSandboxRpcHost(host)).toBe(false);
  });
});
