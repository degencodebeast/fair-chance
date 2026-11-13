"""Tests for scripts/check_readme_links.py. No network: the external and RPC readers are faked."""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_readme_links as c  # noqa: E402


def make(files):
    root = Path(tempfile.mkdtemp())
    for name, text in files.items():
        p = root / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)
    return root


class Slugs(unittest.TestCase):
    def test_github_slug_rules(self):
        self.assertEqual(c.slug("Try it in 90 seconds"), "try-it-in-90-seconds")
        self.assertEqual(c.slug("What `FC_ONE_YOCTO` means (and why)"), "what-fc_one_yocto-means-and-why")
        self.assertEqual(c.slug("Libraries, templates and AI tools"), "libraries-templates-and-ai-tools")

    def test_repeated_headings_get_a_suffix(self):
        anchors = c.anchors("# A\n## B\n## B\n")
        self.assertEqual(anchors, {"a", "b", "b-1"})

    def test_a_heading_inside_a_code_fence_is_not_an_anchor(self):
        self.assertEqual(c.anchors("# A\n```\n# not\n```\n"), {"a"})


class Files(unittest.TestCase):
    def test_a_good_file_and_anchor_pass(self):
        root = make({"README.md": "# T\n[a](docs/x.md#part) [b](#t)\n", "docs/x.md": "## Part\n"})
        self.assertEqual(c.check_local(root, [root / "README.md"]), [])

    def test_a_missing_file_is_reported(self):
        root = make({"README.md": "[a](docs/none.md)\n"})
        out = c.check_local(root, [root / "README.md"])
        self.assertEqual(len(out), 1)
        self.assertIn("docs/none.md", out[0])

    def test_a_missing_anchor_is_reported(self):
        root = make({"README.md": "# T\n[a](#nope)\n"})
        out = c.check_local(root, [root / "README.md"])
        self.assertEqual(len(out), 1)
        self.assertIn("#nope", out[0])

    def test_links_in_code_are_ignored(self):
        root = make({"README.md": "```\n[a](none.md)\n```\n`[b](none.md)`\n"})
        self.assertEqual(c.check_local(root, [root / "README.md"]), [])

    def test_a_folder_link_passes(self):
        root = make({"README.md": "[a](docs/)\n", "docs/x.md": "x\n"})
        self.assertEqual(c.check_local(root, [root / "README.md"]), [])

    def test_the_contents_list_must_link_every_section(self):
        root = make({"README.md": "# T\n## Contents\n- [One](#one)\n## One\n## Two\n"})
        out = c.check_contents(root / "README.md")
        self.assertEqual(len(out), 1)
        self.assertIn("Two", out[0])
        root = make({"README.md": "# T\n## Contents\n- [One](#one)\n- [Two](#two)\n## One\n## Two\n"})
        self.assertEqual(c.check_contents(root / "README.md"), [])


class External(unittest.TestCase):
    def test_each_link_is_asked_once_and_403_and_429_are_reported_not_retried(self):
        root = make({"README.md": "[a](https://ok.example/a) [b](https://slow.example/b) [c](https://gone.example/c)\n"})
        asked = []

        def fetch(url):
            asked.append(url)
            return {"https://ok.example/a": 200, "https://slow.example/b": 429, "https://gone.example/c": 404}[url]

        problems, notes = c.check_external(root, [root / "README.md"], fetch)
        self.assertEqual(len(asked), 3)
        self.assertEqual(len(problems), 1)
        self.assertIn("404", problems[0])
        self.assertEqual(len(notes), 1)
        self.assertIn("429", notes[0])

    def test_one_url_used_twice_is_asked_once(self):
        root = make({"README.md": "[a](https://ok.example/a) [b](https://ok.example/a)\n"})
        asked = []
        c.check_external(root, [root / "README.md"], lambda u: asked.append(u) or 200)
        self.assertEqual(asked, ["https://ok.example/a"])


class Testnet(unittest.TestCase):
    def test_accounts_are_looked_up_and_a_missing_one_is_reported(self):
        root = make({"README.md": "coin `good.fc.testnet` and `bad.fc.testnet`\n"})
        calls = []

        def rpc(method, params):
            calls.append((method, params))
            if params.get("account_id") == "bad.fc.testnet":
                return {"error": {"cause": {"name": "UNKNOWN_ACCOUNT"}}}
            return {"result": {"code_hash": "x"}}

        out = c.check_testnet(root, [root / "README.md"], rpc)
        self.assertEqual(len(out), 1)
        self.assertIn("bad.fc.testnet", out[0])
        self.assertTrue(all(m == "query" for m, _ in calls))

    def test_a_tx_hash_in_a_link_resolves_with_the_sender_on_its_line(self):
        h = "9" * 44
        root = make({"README.md": f"[tx](https://explorer.testnet.near.org/transactions/{h}) from alice.testnet\n"})
        seen = []

        def rpc(method, params):
            seen.append(method)
            if method == "query":
                return {"result": {}}
            return {"result": {"status": {"SuccessValue": ""}}}

        self.assertEqual(c.check_testnet(root, [root / "README.md"], rpc), [])
        self.assertIn("EXPERIMENTAL_tx_status", seen)

    def test_a_tx_hash_with_no_sender_on_its_line_is_reported(self):
        h = "9" * 44
        root = make({"README.md": f"[tx](https://explorer.testnet.near.org/transactions/{h})\n"})
        out = c.check_testnet(root, [root / "README.md"], lambda m, p: {"result": {}})
        self.assertEqual(len(out), 1)
        self.assertIn("sender", out[0])

    def test_a_failed_tx_is_reported(self):
        h = "9" * 44
        root = make({"README.md": f"[tx](https://explorer.testnet.near.org/transactions/{h}) alice.testnet\n"})

        def rpc(method, params):
            if method == "query":
                return {"result": {}}
            return {"result": {"status": {"Failure": {}}}}

        self.assertEqual(len(c.check_testnet(root, [root / "README.md"], rpc)), 1)


if __name__ == "__main__":
    unittest.main()
