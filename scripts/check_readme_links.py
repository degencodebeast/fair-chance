#!/usr/bin/env python3
"""Read-only link check for the README and the docs it links.

  check_readme_links.py                     files, anchors and the Contents list (no network)
  check_readme_links.py --external          also ask each http(s) link once
  check_readme_links.py --testnet           also look up each testnet account and each transaction
                                            hash in a link on the public testnet RPC
  check_readme_links.py FILE...             check these files instead of the default list

It holds no key and sends no transaction. An external link that answers 403 or 429 is
reported and never asked again. Exit 0 when nothing is wrong, 1 otherwise.
"""
import json
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

DEFAULT_FILES = ["README.md", "docs/evidence.md", "docs/model/README.md"]
RPC_URL = "https://rpc.testnet.near.org"
LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)\)")
ACCOUNT = re.compile(r"(?<![A-Za-z0-9_.@-])([a-z0-9][a-z0-9_-]*(?:\.[a-z0-9_-]+)*\.testnet)(?![A-Za-z0-9_-]|\.[A-Za-z0-9])")
TX_HASH = re.compile(r"/(?:transactions|txns|txn)/([1-9A-HJ-NP-Za-km-z]{43,44})\b")


def slug(heading):
    text = heading.strip().lower()
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def prose_lines(text):
    """(line number, line) for every line outside a code fence, with inline code removed."""
    fenced = False
    for number, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("```"):
            fenced = not fenced
            continue
        if not fenced:
            yield number, re.sub(r"`[^`]*`", "", line)


def headings(text):
    for _, line in prose_lines_keep_code(text):
        match = re.match(r"(#{1,6})\s+(.*?)\s*#*\s*$", line)
        if match:
            yield len(match.group(1)), match.group(2)


def prose_lines_keep_code(text):
    fenced = False
    for number, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("```"):
            fenced = not fenced
            continue
        if not fenced:
            yield number, line


def anchors(text):
    seen = {}
    found = set()
    for _, title in headings(text):
        base = slug(title)
        count = seen.get(base, 0)
        seen[base] = count + 1
        found.add(base if count == 0 else f"{base}-{count}")
    return found


def links(path):
    for number, line in prose_lines(path.read_text()):
        for match in LINK.finditer(line):
            yield number, match.group(1)


def check_local(root, files):
    problems = []
    for path in files:
        for number, target in links(path):
            if re.match(r"[a-z][a-z0-9+.-]*:", target):
                continue
            name, _, fragment = target.partition("#")
            dest = path if name == "" else (path.parent / name).resolve()
            where = f"{path.relative_to(root)}:{number}"
            if not dest.exists():
                problems.append(f"{where}: no such file: {target}")
            elif fragment and dest.is_file() and dest.suffix == ".md":
                if fragment not in anchors(dest.read_text()):
                    problems.append(f"{where}: no such anchor: {target}")
    return problems


def check_contents(readme):
    """Every second-level section after Contents must be linked from the Contents list."""
    text = readme.read_text()
    sections = [t for level, t in headings(text) if level == 2]
    if "Contents" not in sections:
        return [f"{readme.name}: no Contents section"]
    block = re.search(r"^##\s+Contents\s*$(.*?)(?=^##\s)", text, re.S | re.M)
    linked = set(re.findall(r"\]\(#([^)\s]+)\)", block.group(1) if block else ""))
    wanted = anchors(text)
    problems = []
    for title in sections:
        if title == "Contents":
            continue
        if slug(title) in wanted and slug(title) not in linked:
            problems.append(f"{readme.name}: Contents does not link the section '{title}'")
    return problems


def fetch_status(url):
    request = urllib.request.Request(url, headers={"User-Agent": "link-check"})
    try:
        with urllib.request.urlopen(request, timeout=15) as reply:
            return reply.status
    except urllib.error.HTTPError as error:
        return error.code
    except Exception as error:  # a network failure is a problem, not a crash
        return f"error {type(error).__name__}"


def check_external(root, files, fetch=fetch_status):
    problems, notes, done = [], [], {}
    for path in files:
        for number, target in links(path):
            if not target.startswith(("http://", "https://")):
                continue
            url = target.split("#")[0]
            if url not in done:
                done[url] = fetch(url)
            status = done[url]
            where = f"{path.relative_to(root)}:{number}: {url}"
            if status in (403, 429):
                notes.append(f"{where}: answered {status}, not retried")
            elif not (isinstance(status, int) and 200 <= status < 400):
                problems.append(f"{where}: {status}")
    return problems, notes


def rpc_call(method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    request = urllib.request.Request(RPC_URL, body, {"content-type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=20) as reply:
            return json.load(reply)
    except Exception as error:
        return {"error": {"message": f"{type(error).__name__}"}}


def check_testnet(root, files, rpc=rpc_call):
    problems, accounts = [], {}
    for path in files:
        text = path.read_text()
        for number, line in prose_lines_keep_code(text):
            where = f"{path.relative_to(root)}:{number}"
            for account in ACCOUNT.findall(line):
                if account not in accounts:
                    reply = rpc("query", {"request_type": "view_account", "finality": "final", "account_id": account})
                    accounts[account] = "result" in reply
                if not accounts[account]:
                    problems.append(f"{where}: testnet account not found: {account}")
            for tx in TX_HASH.findall(line):
                senders = ACCOUNT.findall(line)
                if not senders:
                    problems.append(f"{where}: transaction {tx} has no sender account on its line")
                    continue
                reply = rpc("EXPERIMENTAL_tx_status", {"tx_hash": tx, "sender_account_id": senders[0], "wait_until": "FINAL"})
                status = reply.get("result", {}).get("status")
                if not isinstance(status, dict) or "SuccessValue" not in status and "SuccessReceiptId" not in status:
                    problems.append(f"{where}: transaction {tx} did not resolve as a success")
    return problems


def main(argv):
    flags = [a for a in argv if a.startswith("--")]
    names = [a for a in argv if not a.startswith("--")] or DEFAULT_FILES
    root = Path(__file__).resolve().parent.parent
    files = [(root / n).resolve() for n in names]
    problems = check_local(root, files)
    for readme in files:
        if readme.name == "README.md" and readme.parent == root:
            problems += check_contents(readme)
    notes = []
    if "--external" in flags:
        more, notes = check_external(root, files)
        problems += more
    if "--testnet" in flags:
        problems += check_testnet(root, files)
    for line in notes:
        print(f"note: {line}")
    for line in problems:
        print(f"FAIL: {line}")
    print(f"check-readme-links: {'FAIL' if problems else 'PASS'} ({len(files)} files, {len(problems)} problems, {len(notes)} notes)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
