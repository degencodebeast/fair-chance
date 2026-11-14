#!/usr/bin/env python3
"""Read-only link check for the README and the docs it links.

  check_readme_links.py                     files, anchors and the Contents list (no network)
  check_readme_links.py --external          also ask each http(s) link once
  check_readme_links.py --testnet           also look up each testnet account and each transaction
                                            link on the public testnet RPC and compare the receipt
                                            with the claim in the link title
  check_readme_links.py FILE...             check these files instead of the default list

It holds no key and sends no transaction. An external link that answers 403 or 429 is
reported and never asked again. Exit 0 when nothing is wrong, 1 otherwise, 2 on a bad flag.

A transaction link carries its claim in the link title:
  [tx](https://explorer.../transactions/<hash> "claim: partial_fill by buyer.testnet")
The claim is partial_fill, full_fill, launch or refusal:<FC_CODE>. The account after "by" is the
sender. Without "by", the sender comes from the JSON files in evidence/ (the entry with that hash). The check reads the receipt and compares what it finds with the claim.
"""
import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

DEFAULT_FILES = ["README.md", "docs/evidence.md", "docs/model/README.md"]
RPC_URL = "https://rpc.testnet.near.org"
LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)(?:\s+\"([^\"]*)\")?\)")
CLAIM = re.compile(r"claim:\s*(partial_fill|full_fill|launch|refusal:FC_[A-Z_]+)(?:\s+by\s+(\S+))?")
FC_CODE = re.compile(r"FC_[A-Z_]+")
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


def links_with_titles(path):
    for number, line in prose_lines(path.read_text()):
        for match in LINK.finditer(line):
            yield number, match.group(1), match.group(2) or ""


def exact_case(root, dest):
    """True when every part of the path below root has the case the file system lists."""
    current = root
    for part in dest.relative_to(root).parts:
        if part not in os.listdir(current):
            return False
        current = current / part
    return True


def check_local(root, files):
    problems = []
    for path in files:
        for number, target in links(path):
            if re.match(r"[a-z][a-z0-9+.-]*:", target):
                continue
            name, _, fragment = target.partition("#")
            dest = path.resolve() if name == "" else (path.parent / name).resolve()
            where = f"{path.relative_to(root)}:{number}"
            if not dest.is_relative_to(root.resolve()):
                problems.append(f"{where}: link leaves the repository: {target}")
            elif not dest.exists() or not exact_case(root.resolve(), dest):
                problems.append(f"{where}: no such file: {target}")
            elif fragment and dest.is_file() and dest.suffix.lower() == ".md":
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


def receipt_outcome(reply):
    """What a transaction did, as the claim words: partial_fill, full_fill, no_fill,
    refusal:FC_CODE, or None when the reply holds no result."""
    result = reply.get("result")
    if not isinstance(result, dict):
        return None
    outcomes = [r.get("outcome", {}) for r in result.get("receipts_outcome", []) if isinstance(r, dict)]
    for outcome in outcomes:
        for log in outcome.get("logs", []):
            if not log.startswith("EVENT_JSON:"):
                continue
            try:
                event = json.loads(log[len("EVENT_JSON:"):])
            except ValueError:
                continue
            if event.get("event") == "fc_launch":
                return "launch"
            if event.get("event") == "fc_sell":
                fill = (event.get("data") or [{}])[0].get("fill")
                return {"partial": "partial_fill", "full": "full_fill", "none": "no_fill"}.get(fill)
    for outcome in outcomes + [{"status": result.get("status")}]:
        status = outcome.get("status")
        if isinstance(status, dict) and "Failure" in status:
            code = FC_CODE.search(json.dumps(status["Failure"]))
            return f"refusal:{code.group()}" if code else "failure"
    return None


def evidence_senders(root):
    """Transaction hash -> sender account, read from the JSON files in evidence/. An entry with
    "tx" or "tx_hash" and "account" gives its account. A "launch" entry with no account of its
    own belongs to the top-level "creator"."""
    senders = {}

    def walk(node, creator):
        if isinstance(node, dict):
            for key in ("tx", "tx_hash"):
                if isinstance(node.get(key), str) and isinstance(node.get("account"), str):
                    senders[node[key]] = node["account"]
            for key, child in node.items():
                if key == "launch" and isinstance(child, dict) and isinstance(creator, str):
                    for tx_key in ("tx", "tx_hash"):
                        if isinstance(child.get(tx_key), str):
                            senders.setdefault(child[tx_key], child.get("account", creator))
                walk(child, creator)
        elif isinstance(node, list):
            for child in node:
                walk(child, creator)

    folder = root / "evidence"
    for path in sorted(folder.glob("*.json")) if folder.is_dir() else []:
        try:
            data = json.loads(path.read_text())
        except ValueError:
            continue
        walk(data, data.get("creator") if isinstance(data, dict) else None)
    return senders


def check_testnet(root, files, rpc=rpc_call):
    problems, accounts = [], {}
    senders = evidence_senders(root)
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
        for number, target, title in links_with_titles(path):
            tx = TX_HASH.search(target)
            if not tx:
                continue
            where = f"{path.relative_to(root)}:{number}"
            claim = CLAIM.search(title)
            if not claim:
                problems.append(f"{where}: transaction {tx.group(1)} has no claim in its link title")
                continue
            sender = claim.group(2) or senders.get(tx.group(1))
            if not sender:
                problems.append(f"{where}: transaction {tx.group(1)} has no sender in its claim or in evidence/")
                continue
            reply = rpc("EXPERIMENTAL_tx_status", {"tx_hash": tx.group(1), "sender_account_id": sender, "wait_until": "FINAL"})
            found = receipt_outcome(reply)
            if found != claim.group(1):
                problems.append(f"{where}: transaction {tx.group(1)} claims {claim.group(1)} but the receipt shows {found}")
    return problems


def parse(argv):
    parser = argparse.ArgumentParser(description="Read-only link check for the README and its docs.")
    parser.add_argument("--external", action="store_true", help="ask each http(s) link once")
    parser.add_argument("--testnet", action="store_true", help="look up testnet accounts and transactions")
    parser.add_argument("files", nargs="*", help="files to check (default: the README and two docs)")
    return parser.parse_args(argv)


def main(argv):
    args = parse(argv)
    root = Path(__file__).resolve().parent.parent
    files = [(root / n).resolve() for n in (args.files or DEFAULT_FILES)]
    problems = check_local(root, files)
    for readme in files:
        if readme.name == "README.md" and readme.parent == root:
            problems += check_contents(readme)
    notes = []
    if args.external:
        more, notes = check_external(root, files)
        problems += more
    if args.testnet:
        problems += check_testnet(root, files)
    for line in notes:
        print(f"note: {line}")
    for line in problems:
        print(f"FAIL: {line}")
    print(f"check-readme-links: {'FAIL' if problems else 'PASS'} ({len(files)} files, {len(problems)} problems, {len(notes)} notes)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
