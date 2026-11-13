// Two host tests, kept apart on purpose. `hostname` is as URL gives it: "localhost", "app.localhost",
// "127.0.0.5", "0.0.0.0" or "[::1]".

/** The visitor's own machine, in any spelling. Used only to skip the hosted redirect. */
export function isOwnMachineHost(hostname: string): boolean {
  const host = hostname.toLowerCase();
  if (host === "localhost" || host.endsWith(".localhost")) return true;
  if (host === "[::1]" || host === "0.0.0.0") return true;
  return /^127\.(?:\d{1,3})\.(?:\d{1,3})\.(?:\d{1,3})$/.test(host);
}

/** The one pair of exact hosts a page link may give as the RPC URL: a local sandbox. */
export function isSandboxRpcHost(hostname: string): boolean {
  return hostname === "localhost" || hostname === "127.0.0.1";
}
