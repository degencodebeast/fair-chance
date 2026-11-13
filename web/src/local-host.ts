// The one test for "this host is the visitor's own machine". The hosted entry (no redirect) and the
// page link (a local sandbox may give the RPC URL) both use it, so the two never disagree.
/** `hostname` as URL gives it: "localhost", "app.localhost", "127.0.0.5", "0.0.0.0" or "[::1]". */
export function isLocalHostName(hostname: string): boolean {
  const host = hostname.toLowerCase();
  if (host === "localhost" || host.endsWith(".localhost")) return true;
  if (host === "[::1]" || host === "0.0.0.0") return true;
  return /^127\.(?:\d{1,3})\.(?:\d{1,3})\.(?:\d{1,3})$/.test(host);
}
