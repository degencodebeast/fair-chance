// Two host tests, kept apart on purpose. `hostname` is as URL gives it: "localhost", "app.localhost",
// "127.0.0.5", "0.0.0.0" or "[::1]".

function isLoopbackV4(first: number): boolean {
  return first === 127;
}

/** The visitor's own machine, in any spelling. Used only to skip the hosted redirect. */
export function isOwnMachineHost(hostname: string): boolean {
  // One trailing dot names the same host ("localhost.").
  const host = hostname.toLowerCase().replace(/\.$/, "");
  if (host === "localhost" || host.endsWith(".localhost")) return true;
  if (host === "[::1]" || host === "0.0.0.0") return true;
  const v4 = /^(\d{1,3})\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.exec(host);
  if (v4) return isLoopbackV4(Number(v4[1]));
  // An IPv4 address inside IPv6: URL writes [::ffff:127.0.0.1] as [::ffff:7f00:1].
  const mapped = /^\[::ffff:([0-9a-f]{1,4}):[0-9a-f]{1,4}\]$/.exec(host);
  if (mapped) return isLoopbackV4(parseInt(mapped[1]!, 16) >> 8);
  const dotted = /^\[::ffff:(\d{1,3})\.\d{1,3}\.\d{1,3}\.\d{1,3}\]$/.exec(host);
  return dotted !== null && isLoopbackV4(Number(dotted[1]));
}

/** The one pair of exact hosts a page link may give as the RPC URL: a local sandbox. */
export function isSandboxRpcHost(hostname: string): boolean {
  return hostname === "localhost" || hostname === "127.0.0.1";
}
