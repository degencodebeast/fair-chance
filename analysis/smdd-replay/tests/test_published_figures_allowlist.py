"""The guard's exact allowlist: it reads the testnet run record and passes only the exact values
in it (the coin code hash, the .testnet factory and coin accounts, the recorded transaction hashes).
Every file here is made up for the test.
"""
import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from published_figures_guard import findings_in_text, load_allowlist  # noqa: E402

CODE_HASH = "Dq3bY8mHfA2xKpL9sT7vWcN4rUeJ6gZ1hXnB5aMiQyRk"
MADE_UP_HASH = "Dq3bY8mHfA2xKpL9sT7vWcN4rUeJ6gZ1hXnB5aMiQyRm"
TX_HASH = "7mPqRs2tUvWxYz3AbCdEfGhJkLnNpQrStUvWxYz4Ab5C"
COMMIT = "0123456789abcdef0123456789abcdef01234567"
FACTORY = "fc-demo.testnet"
COIN = "bag.fc-demo.testnet"
OTHER_HASH = "Zx9cV8bN7mK6jH5gF4dS3aQ2wE1rT0yUiOpLkJhGfDsA"
RECORD = {
    "network": "testnet",
    "deployed_commit": COMMIT,
    "factory": FACTORY,
    "coin": COIN,
    "code_hash": CODE_HASH,
    "note": OTHER_HASH,
    "transactions": [{"tx_hash": TX_HASH, "outcome": "partial_fill", "account": "buyer1.testnet", "memo": OTHER_HASH}],
}


def record_file(data=RECORD):
    folder = Path(tempfile.mkdtemp())
    path = folder / "testnet-run.json"
    path.write_text(json.dumps(data))
    return path


def findings(text, allowed):
    return findings_in_text("caption.txt", text + "\n", allowed=allowed)


class AllowlistTest(unittest.TestCase):
    def setUp(self):
        self.allowed = load_allowlist(record_file())

    def test_the_recorded_code_hash_passes_and_fails_without_the_allowlist(self):
        text = f"The coin runs code hash {CODE_HASH}."
        self.assertEqual(findings(text, self.allowed), [])
        self.assertEqual(len(findings(text, frozenset())), 1)

    def test_a_made_up_hash_of_the_same_shape_fails(self):
        self.assertEqual(len(findings(f"The coin runs code hash {MADE_UP_HASH}.", self.allowed)), 1)

    def test_a_recorded_transaction_hash_passes(self):
        self.assertEqual(findings(f"The partial fill is in transaction {TX_HASH}.", self.allowed), [])

    def test_the_recorded_testnet_accounts_pass_and_others_fail(self):
        self.assertEqual(findings(f"The factory is {FACTORY} and the coin is {COIN}.", self.allowed), [])
        self.assertEqual(len(findings("The coin is other.testnet.", self.allowed)), 1)

    def test_a_mainnet_style_account_fails(self):
        self.assertEqual(len(findings("The coin is bag.near.", self.allowed)), 1)

    def test_a_hash_shaped_string_in_another_field_does_not_pass(self):
        self.assertEqual(len(findings(f"Hash {OTHER_HASH}.", self.allowed)), 1)

    def test_an_account_in_a_transaction_row_does_not_pass(self):
        self.assertEqual(len(findings("The buyer is buyer1.testnet.", self.allowed)), 1)

    def test_the_commit_in_the_record_is_not_allowed(self):
        self.assertEqual(len(findings(f"Built from commit {COMMIT}.", self.allowed)), 1)

    def test_an_allowed_value_inside_a_longer_word_fails(self):
        self.assertEqual(len(findings(f"Code hash x{CODE_HASH}.", self.allowed)), 1)
        self.assertEqual(len(findings(f"Account x{COIN}.", self.allowed)), 1)

    def test_the_other_rules_still_apply_next_to_an_allowed_value(self):
        self.assertEqual(len(findings(f"The creator sold at 14:05 in {TX_HASH}.", self.allowed)), 1)
        self.assertEqual(len(findings(f"One buyer sold 40% in {TX_HASH}.", self.allowed)), 1)

    def test_an_allowed_account_in_a_table_row_is_still_not_a_group(self):
        table = f"| Group | Account |\n| --- | --- |\n| {COIN} | x |\n"
        self.assertTrue(findings_in_text("results.md", table, allowed=self.allowed))


class NetworkTest(unittest.TestCase):
    def test_a_mainnet_file_gives_an_empty_allowlist(self):
        self.assertEqual(load_allowlist(record_file(dict(RECORD, network="mainnet"))), frozenset())

    def test_a_file_with_no_network_gives_an_empty_allowlist(self):
        data = {k: v for k, v in RECORD.items() if k != "network"}
        self.assertEqual(load_allowlist(record_file(data)), frozenset())


class MissingFileTest(unittest.TestCase):
    def test_a_missing_file_gives_an_empty_allowlist(self):
        self.assertEqual(load_allowlist(Path(tempfile.mkdtemp()) / "none.json"), frozenset())

    def test_a_missing_file_keeps_the_old_behaviour(self):
        allowed = load_allowlist(Path(tempfile.mkdtemp()) / "none.json")
        self.assertEqual(len(findings(f"Code hash {CODE_HASH}.", allowed)), 1)
        self.assertEqual(findings("Minute 12 after launch.", allowed), [])

    def test_a_file_that_is_not_json_gives_an_empty_allowlist(self):
        path = record_file()
        path.write_text("not json")
        self.assertEqual(load_allowlist(path), frozenset())


if __name__ == "__main__":
    unittest.main()
