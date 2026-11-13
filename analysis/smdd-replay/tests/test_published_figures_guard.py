"""The guard over every published output, caption and video script: each rule fails on a bad
made-up text and passes on a good one. No text here comes from a trade file.
"""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from published_figures_guard import findings_in_files, findings_in_text  # noqa: E402

GUARD = HERE.parent / "published_figures_guard.py"
TABLE = "| Group | Tokens sold |\n| --- | --- |\n"


def findings(text, name="caption.txt"):
    return findings_in_text(name, text)


class ClockTimeTest(unittest.TestCase):
    def test_an_absolute_clock_time_fails(self):
        for bad in ("The creator sold at 14:05.", "It ended at 09:31:07 UTC.", "Launch: 2024-11-18T21:04Z.",
                    "The sell came at 9pm."):
            self.assertEqual(len(findings(bad + "\n")), 1, bad)

    def test_minutes_and_blocks_after_launch_pass(self):
        for good in ("Minute 12 after launch.", "Block 17 after launch.", "In the first hour."):
            self.assertEqual(findings(good + "\n"), [], good)


class LongCodeTest(unittest.TestCase):
    def test_a_long_base58_or_hex_string_fails(self):
        for bad in (
            "Seen in Fixture1Base58StringNotAnAddressAbcdefghijkmnop today.",
            "Hash 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08 only.",
            "Account 0xabababababababababababababababababababab bought.",
        ):
            self.assertEqual(len(findings(bad + "\n")), 1, bad)

    def test_short_codes_and_words_pass(self):
        self.assertEqual(findings("FC_EARLY_WINDOW refused it; see analysis/smdd-replay/METHOD.md.\n"), [])


class VenueTest(unittest.TestCase):
    def test_a_venue_word_fails(self):
        for bad in ("It moved to Raydium.", "Bought on pump.fun.", "After the curve filled.",
                    "Before the migration.", "Once it graduated."):
            self.assertEqual(len(findings(bad + "\n")), 1, bad)

    def test_the_builders_creator_line_may_name_the_launch_curve(self):
        line = (
            "Real coin: the creator sold 7.331% of supply. Replay with limits off: 6.77%. The gap "
            "comes from the curve shape (pump.fun's 1.073 billion virtual tokens).\n"
        )
        self.assertEqual(findings(line), [])


class TradeSizeTest(unittest.TestCase):
    def test_an_amount_with_a_unit_fails(self):
        for bad in ("It paid 2.5 SOL.", "The sell gave 0.49 NEAR.", "It bought 4.9M tokens.",
                    "It sent 1,200,000,000 lamports."):
            self.assertEqual(len(findings(bad + "\n")), 1, bad)

    def test_shares_pass(self):
        self.assertEqual(findings("The creator sold 12.3% of all tokens sold in that run.\n"), [])


class SingleTraderAtASecondTest(unittest.TestCase):
    def test_prose_about_one_buyer_or_seller_at_a_second_fails(self):
        for bad in ("A buyer came in 2 seconds after launch.", "One seller left at second 45.",
                    "The wallet bought 3 s after launch."):
            self.assertEqual(len(findings(bad + "\n")), 1, bad)

    def test_the_creator_and_groups_pass(self):
        for good in ("The creator bought 0 seconds after launch.",
                     "Buyers in the first minute paid the early fee."):
            self.assertEqual(findings(good + "\n"), [], good)


class PerWalletTest(unittest.TestCase):
    def test_a_per_wallet_row_list_or_example_fails(self):
        for bad in (
            TABLE + "| other-3 | 1.2% |\n",
            "The top 5 sellers took most of it.\n",
            "The largest buyer held on.\n",
            "For example, one account sold half its tokens.\n",
        ):
            self.assertEqual(len(findings(bad)), 1, bad)

    def test_group_rows_pass(self):
        rows = "".join(
            f"| {group} | 1.2% |\n"
            for group in ("creator", "funded directly by the same wallet before launch",
                          "bought 1% of supply or more in the first hour", "other")
        )
        self.assertEqual(findings(TABLE + rows), [])


class MethodFileTest(unittest.TestCase):
    """METHOD.md describes the method, so it may name the method's venues. Every other rule holds."""

    def write(self, text):
        method = Path(tempfile.mkdtemp()) / "METHOD.md"
        method.write_text(text)
        return method

    def test_the_method_file_may_name_the_venues(self):
        method = self.write("A buy takes the same share of the pump.fun curve or pool depth as of its venue.\n")
        self.assertEqual(findings_in_files([method]), [])

    def test_every_other_rule_holds_in_the_method_file(self):
        for bad in ("Launched at 21:04.", "It paid 2.5 SOL.", "A seller left at second 45.",
                    "See other-3.", "Account 0xabababababababababababababababababababab."):
            self.assertEqual(findings_in_files([self.write(bad + "\n")]), ["METHOD.md: line 1"], bad)

    def test_a_caption_may_not_name_a_venue(self):
        caption = Path(tempfile.mkdtemp()) / "captions.txt"
        caption.write_text("A buy takes the same share of the pump.fun curve.\n")
        self.assertEqual(findings_in_files([caption]), ["captions.txt: line 1"])

    def test_the_repositorys_method_file_passes(self):
        self.assertEqual(findings_in_files([HERE.parent / "METHOD.md"]), [])


class FindingTextTest(unittest.TestCase):
    def test_a_finding_names_the_file_line_and_column_and_never_the_text(self):
        (finding,) = findings("| Group | Note |\n| --- | --- |\n| other | sold at 14:05 |\n", "table.md")
        self.assertEqual(finding, "table.md: line 3, column 2")

    def test_a_bad_first_cell_or_header_is_not_printed(self):
        code = "Fixture1Base58StringNotAnAddressAbcdefghijkmnop"
        table = f"| Group | {code} |\n| --- | --- |\n| {code} | 4% |\n"
        found = findings(table, "t.md")
        self.assertEqual(found, ["t.md: line 1, column 2", "t.md: line 3, column 1"])
        self.assertNotIn(code, " ".join(found))

    def test_a_finding_outside_a_table_names_the_line(self):
        self.assertEqual(findings("intro\nIt paid 2.5 SOL.\n", "script.txt"), ["script.txt: line 2"])

    def test_in_a_chart_only_the_visible_text_counts(self):
        folder = Path(tempfile.mkdtemp())
        chart = folder / "chart.svg"
        chart.write_text('<svg><polyline points="40.0,14.05 41,9"/><text x="8">price</text></svg>\n')
        self.assertEqual(findings_in_files([chart]), [])
        chart.write_text('<svg><text x="8">sold at 14:05</text></svg>\n')
        self.assertEqual(findings_in_files([chart]), ["chart.svg: line 1"])

    def test_every_visible_or_accessible_svg_text_counts(self):
        bad = "sold at 21:30:05"
        chart = Path(tempfile.mkdtemp()) / "chart.svg"
        for body in (
            f"<text><tspan>{bad}</tspan></text>",
            f'<text><textPath href="#p">{bad}</textPath></text>',
            f"<text>x<tspan>y</tspan>{bad}</text>",
            f"<desc>{bad}</desc>",
            f"<metadata>{bad}</metadata>",
            f"<!-- {bad} -->",
            f"<text><![CDATA[{bad}]]></text>",
            f'<rect aria-label="{bad}"/>',
            f"<rect data-note='{bad}'/>",
            f"<TEXT>{bad}</TEXT>",
            "<text>sold at 14&#58;05</text>",
            "<text>sold at &#50;1:30</text>",
            "<text>sold at 21&#x3A;30</text>",
            f"<text>\nsold at\n21:30\n</text>",
        ):
            chart.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="720">{body}</svg>\n')
            self.assertNotEqual(findings_in_files([chart]), [], body)


def assert_each_fails(test, texts):
    for bad in texts:
        test.assertNotEqual(findings(bad + "\n"), [], bad)


class NormalizedTextTest(unittest.TestCase):
    """Case, separators and unicode digits do not hide a rule."""

    def test_clock_times_in_any_form_fail(self):
        assert_each_fails(self, (
            "Launch at 14h05 UTC.", "Launch at 14.05.32 UTC.", "Launch at 1405Z.", "Launch at T14:05",
            "Launch at \uff11\uff14:\uff10\uff15", "Launch at \u0662\u0661:\u0660\u0665",
            "Launch at 2026-03-01T14:05:32Z", "Launch at 14 : 05", "Launch at 9 p.m.", "Launch at 21h.",
            "Trading began at 21h30 UTC.", "One block carried unix time 1709301932.",
        ))

    def test_amounts_in_any_unit_fail(self):
        assert_each_fails(self, (
            "One trade of 4.2 wSOL.", "One trade of 4.2 WSOL.", "One trade of 4.2 \u25ce.",
            "One trade of 4.2 \u24c3.", "One trade of 4.2SOLANA.", "One trade of 5,000 USDC.",
            "One trade of $612.", "One trade of 12.3M SMDD.", "One trade of 12.3 million coins.",
            "One trade of four point two SOL.", "One trade of 1 2 3 4 5 SOL.",
        ))

    def test_one_trader_at_a_time_or_with_a_share_fails(self):
        assert_each_fails(self, (
            "A buyer at 0.2 minutes bought 4%.", "A buyer in block 3 bought 4% of supply.",
            "A wallet in minute 0 bought 4% of supply.", "Buyer #7 bought 4% of supply.",
            "The first buyer paid at second 3.", "Two buyers: one bought 4%, one sold 3% at second 12.",
            "Wallet 7 held on.", "Then alice.near sold.", "Say a wallet bought 4% at minute 2.",
            "One wallet bought 4.21% of supply at minute 0 and sold it all at minute 1.",
            "The creator watched while a buyer sold everything at second 5.",
        ))

    def test_rankings_fail(self):
        assert_each_fails(self, (
            "The biggest buy was 4% of supply.", "The largest single purchase was 4% of supply.",
            "Top ten wallets held 40%.", "top10 wallets held 40%.", "Top-ten buyers.",
            "The 3 largest buyers held 40%.",
        ))

    def test_venue_words_in_any_spelling_fail(self):
        assert_each_fails(self, (
            "It began on Pump fun.", "It began on pump-fun.", "It began on PUMP FUN.", "It moved to Ray dium.",
            "The bonding curv filled.", "The bonding-curve filled.", "Two curves.", "Routed by Jupiter.",
            "Pooled on Meteora.", "Pooled on Orca.", "Once it bonded.",
        ))

    def test_addresses_in_any_spelling_fail(self):
        assert_each_fails(self, (
            "Address Fixture0Made0Up0Code0With0Zero0And0O.",
            "Address Fixture7Made8Up9 Code9Split0By1A2Space3.",
            "Hash 0x" + "ab" * 15 + "_" + "cd" * 15 + ".",
        ))

    def test_account_names_in_any_spelling_fail(self):
        assert_each_fails(self, ("See other-01.", "See other_1.", "See o-1.", "See OTHER-3."))


class RawCountTest(unittest.TestCase):
    """A number of five or more digits, with or without separators, fails. Years, small block or
    minute numbers and decimals pass."""

    def test_a_raw_count_fails(self):
        assert_each_fails(self, (
            "The wallet that bought 12,345,678 of supply then sold.", "It held 12345678.", "It held 12 345 678.",
            "It held 12_345.", "It held 98765 of it.", "It held 12.345.678 of it.",
        ))

    def test_years_small_numbers_and_decimals_pass(self):
        for good in ("Launched in 2024.", "Block 17 after launch.", "Minute 59.", "One block every 0.5789 s.",
                     "The creator sold 12.3% of supply.", "It held 1,234 of them."):
            self.assertEqual(findings(good + "\n"), [], good)

    def test_a_raw_count_fails_in_the_method_file_and_in_chart_text_but_not_in_drawing_attributes(self):
        folder = Path(tempfile.mkdtemp())
        method = folder / "METHOD.md"
        method.write_text("Measured over 156,266 blocks.\n")
        self.assertEqual(findings_in_files([method]), ["METHOD.md: line 1"])
        chart = folder / "chart.svg"
        chart.write_text('<svg><polyline points="12345.0,99999.5"/><text>price</text></svg>\n')
        self.assertEqual(findings_in_files([chart]), [])
        chart.write_text("<svg><text>held 12345</text></svg>\n")
        self.assertEqual(findings_in_files([chart]), ["chart.svg: line 1"])


class TableRowsTest(unittest.TestCase):
    """Every header cell is checked, and every row that is not a group fails, wherever it sits."""

    def test_a_header_cell_is_checked(self):
        self.assertEqual(findings("| Group | at 14:05 UTC |\n| --- | --- |\n| creator | 1% |\n", "t.md"),
                         ["t.md: line 1, column 2"])

    def test_rows_of_a_table_with_no_separator_must_be_groups(self):
        self.assertEqual(findings("| wallet A | 4.2% |\n| other | 3.9% |\n", "t.md"), ["t.md: line 1, column 1"])

    def test_a_row_before_or_after_a_second_separator_must_be_a_group(self):
        table = TABLE + "| creator | 1% |\n| buyer at minute 0 | 4.2% |\n| --- | --- |\n| wallet B | 1% |\n"
        self.assertEqual(findings(table, "t.md"), ["t.md: line 4, column 1", "t.md: line 6, column 1"])

    def test_the_readme_row_passes(self):
        self.assertEqual(findings("| SMDD replay | The creator sold 6.7% of supply. | Groups only. |\n"), [])


class GuardCommandTest(unittest.TestCase):
    def run_guard(self, *files):
        return subprocess.run([sys.executable, "-I", str(GUARD), *map(str, files)], capture_output=True, text=True)

    def test_a_caption_file_with_a_clock_time_fails_and_shows_no_text(self):
        caption = Path(tempfile.mkdtemp()) / "captions.txt"
        caption.write_text("The sell came at 14:05.\n")
        result = self.run_guard(caption)
        self.assertEqual(result.returncode, 1)
        self.assertIn("captions.txt: line 1", result.stdout + result.stderr)
        self.assertNotIn("14:05", result.stdout + result.stderr)

    def test_a_clean_video_script_passes(self):
        script = Path(tempfile.mkdtemp()) / "video-script.txt"
        script.write_text("This is a replay of the first hour, by group.\n")
        result = self.run_guard(script)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("passed", result.stdout)

    def test_no_file_named_is_an_error(self):
        self.assertEqual(self.run_guard().returncode, 2)


if __name__ == "__main__":
    unittest.main()
