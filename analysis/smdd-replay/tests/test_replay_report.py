"""The replay's published outputs from the synthetic fixture: what they must say and never show.
The fixture is a fixture; these outputs are never replay output.
"""
import collections
import csv
import functools
import re
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from limits_off_pool import LimitsOffPool  # noqa: E402
from limits_off_replay import Run, Step, play_limits_off  # noqa: E402
from limits_on_replay import InProcessCoinRunner, play_limits_on  # noqa: E402
from published_figures_guard import findings_in_text, visible_text  # noqa: E402
from replay_report import (  # noqa: E402
    same_funder_line,
    CREATOR_FORMS,
    GROUP_NOT_SHOWN_LINE,
    MIN_GROUP_SIZE,
    table,
    PRECISION_LEVELS,
    CreatorForm,
    Precision,
    fill_counts_sentence,
    first_passing,
    render,
    shown_share,
)
from creator_share_check import check_creator_share  # noqa: E402
from trade_file import read_trades  # noqa: E402

FIXTURE = HERE.parent / "fixtures" / "synthetic-trades-fixture.csv"
RUNNER_WORDS = "the same coin code, in the near-sdk in-process runner, with exact blocks and times"
LIMITS_ON_LABEL = "all rules together (early fee, allowance, class and cap)"
TWO_DECIMALS = CreatorForm("share", decimals=2)
THREE_DECIMALS = CreatorForm("share", decimals=3)
RATIO = CreatorForm("ratio", decimals=2)
COUNTS = CreatorForm("counts")
NOT_SHOWN = CreatorForm("not shown")
ONE_DECIMAL = Precision(decimals=1, of_run_totals=True, creator=TWO_DECIMALS)
NOT_SHOWN_LINE = "Not shown: this figure does not pass the privacy guard in any form."


def builder_group_lines_removed(text):
    """The text without the same-funder counts line and the direct-funding line."""
    text = re.sub(r"\d+ wallets funded directly by one wallet before launch paid .*?sold none in the first hour\.", "", text)
    return text.replace(
        'The same-funder check sees direct funding only, so wallets funded through other wallets can sit in "other".', ""
    )


@functools.cache
def fixture_runs():
    """Both runs of the fixture and its creator check, played once for all tests."""
    runner = InProcessCoinRunner()
    rules, supply = runner.rules()
    trades = read_trades(FIXTURE)
    limits_off = play_limits_off(
        trades,
        LimitsOffPool.from_rules(rules, supply, int(rules["creator_fee_share_bps"]), early_fee=False),
    )
    return limits_off, play_limits_on(trades, runner), check_creator_share(limits_off, FIXTURE)


class ReplayReportTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.limits_off, cls.limits_on, cls.creator = fixture_runs()
        cls.texts = render(cls.limits_off, cls.limits_on, ONE_DECIMAL, cls.creator)
        cls.table = cls.texts["replay-who-sold.md"]
        cls.all_text = "\n".join(cls.texts.values())

    def rows(self):
        lines = [l for l in self.table.splitlines() if l.startswith("|") and "---" not in l]
        return [[c.strip() for c in l.strip().strip("|").split("|")] for l in lines]

    def test_the_outputs_are_a_table_a_chart_a_one_line_summary_and_the_readme_section(self):
        self.assertEqual(
            sorted(self.texts),
            ["readme-section.md", "replay-price-path.svg", "replay-summary.txt", "replay-who-sold.md"],
        )
        self.assertEqual(len(self.texts["replay-summary.txt"].strip().splitlines()), 1)

    def test_every_output_says_it_is_a_replay_and_names_no_wallet_and_no_person(self):
        for name, text in self.texts.items():
            self.assertIn("replay", text.lower(), name)
            # The group label, the group's counts line and the direct-funding line name wallets as a
            # group, by the builder's rule; no other text names a wallet.
            shown = builder_group_lines_removed(text).lower()
            shown = shown.replace("funded directly by the same wallet before launch", "")
            for word in ("wallet", "builder", "my trade", "our trade"):
                self.assertIsNone(re.search(rf"\b{word}\b", shown), name)

    def test_every_fixture_row_is_played_in_both_runs_per_role(self):
        with FIXTURE.open(newline="") as file:
            rows_in = collections.Counter(
                "same-funder" if r["role"].startswith("same-funder-") else r["role"]
                for r in csv.DictReader(file)
            )
        file_role = {
            "creator": "creator",
            "funded directly by the same wallet before launch": "same-funder",
            "bought 1% of supply or more in the first hour": "large",
            "other": "other",
        }
        for run in (self.limits_off, self.limits_on):
            self.assertEqual(collections.Counter(file_role[s.group] for s in run.steps), rows_in)

    def test_tokens_sold_and_near_paid_out_are_shares_of_each_runs_own_totals(self):
        header, *body = self.rows()
        self.assertEqual(
            header[1:5],
            [
                "Tokens sold, limits off (share of all tokens sold in that run)",
                "NEAR paid out, limits off (share of all NEAR paid out in that run)",
                "Tokens sold, limits on (share of all tokens sold in that run)",
                "NEAR paid out, limits on (share of all NEAR paid out in that run)",
            ],
        )
        for row in body:
            # The creator's limits-on cells have their own precision (CreatorLimitsOnCellTest).
            cells = row[1:3] if row[0] == "creator" else row[1:5]
            for cell in cells:
                self.assertRegex(cell, r"^(\d+\.\d%|0%|below 0\.1%|Not shown)$")
        self.assertIn("one total for the limits-off run, another for the limits-on run", self.table)
        self.assertIn("the shares are not amounts", self.table)
        self.assertNotIn("share of supply", self.table)

    def test_each_share_column_adds_up_to_at_most_about_one_hundred(self):
        # A group cell that is not shown (SmallGroupTest) leaves the column short of 100.
        header, *body = self.rows()
        for column in (1, 2, 3, 4):
            shown = [r[column] for r in body if r[column] != "Not shown"]
            total = sum(float(c[:-1]) for c in shown if not c.startswith("below"))
            self.assertLessEqual(total, 100.3)

    def test_the_readme_section_gives_the_creator_check_and_names_only_the_curve_shape(self):
        # The fixture's creator sold 67,062,500 of 1e9 tokens: 6.706% of supply.
        line = re.search(r"Real coin: .*\n", self.texts["readme-section.md"]).group()
        self.assertRegex(
            line,
            r"^Real coin: the creator sold 6\.706% of supply\. Replay with limits off: \d+\.\d\d%\. "
            r"The gap comes from the curve shape \(pump\.fun's 1\.073 billion virtual tokens\)\.\n$",
        )

    def test_no_text_gives_the_early_window_in_seconds(self):
        for wrong in ("17 seconds", "18 seconds"):
            self.assertNotIn(wrong, self.all_text)

    def test_no_text_names_the_swap_fee_as_a_cause_of_the_gap(self):
        self.assertNotIn("fee", re.search(r"Real coin: .*", self.texts["readme-section.md"]).group())
        self.assertNotIn("swap fee", self.creator.sentence())

    def test_the_last_fallback_shows_shares_of_supply_only(self):
        texts = render(
            self.limits_off, self.limits_on, Precision(1, of_run_totals=False, creator=TWO_DECIMALS), self.creator
        )
        header = [c.strip() for c in texts["replay-who-sold.md"].splitlines()[2].strip("|").split("|")]
        self.assertEqual(
            header[1:3],
            ["Tokens sold, limits off (share of supply)", "Tokens sold, limits on (share of supply)"],
        )
        self.assertNotIn("NEAR paid out", texts["replay-who-sold.md"])

    def test_the_chart_holds_words_only(self):
        words = re.findall(r">([^<]*)<", self.texts["replay-price-path.svg"])
        self.assertTrue(words)
        self.assertFalse(any(re.search(r"\d", w) for w in words), words)

    def test_the_price_path_has_every_minute_of_the_hour(self):
        # In the fixture minutes 11 to 30 and 32 to 45 hold no trade at all.
        # hold no kept trade at all. An empty minute keeps the price of the minute before it.
        for run in (self.limits_off, self.limits_on):
            points = run.price_by_minute
            self.assertEqual([minute for minute, _ in points], list(range(60)))
            traded = {step.minute for step in run.steps}
            for (minute, price), (_, before) in zip(points[1:], points):
                if minute not in traded:
                    self.assertEqual(price, before, minute)
        for polyline in re.findall(r'points="([^"]*)"', self.texts["replay-price-path.svg"]):
            self.assertEqual(len(polyline.split()), 60)

    def test_no_output_names_a_minute(self):
        # Minutes appear only as chart positions; no text shows a minute that could point to a gap.
        for name in ("replay-who-sold.md", "replay-summary.txt", "readme-section.md"):
            self.assertIsNone(re.search(r"\bminute \d|\d+ min\b", self.texts[name]), name)

    def test_the_runs_are_labelled_and_the_runner_is_named_and_never_sandbox(self):
        for name in ("replay-who-sold.md", "replay-summary.txt", "readme-section.md"):
            self.assertIn(LIMITS_ON_LABEL, self.texts[name], name)
            self.assertNotIn("sandbox", self.texts[name].lower(), name)
        self.assertIn(RUNNER_WORDS, self.table)
        self.assertIn("no Fair Chance rule", self.table)
        self.assertIn("not from one of them", self.table)

    def test_the_table_states_how_a_trade_is_sized_and_the_block_time_source(self):
        self.assertIn("same share of the Fair Chance pool's NEAR depth", self.table)
        self.assertIn("0.5789 seconds per block", self.table)
        self.assertIn("analysis/near-replay/fixtures", self.table)

    def test_classes_and_refusals_come_from_the_rule_engine(self):
        one_percent = next(r for r in self.rows() if r[0].startswith("bought 1% of supply"))
        self.assertEqual(one_percent[0], "bought 1% of supply or more in the first hour")
        creator = next(r for r in self.rows() if r[0] == "creator")
        self.assertIn("class large", creator[-1])
        self.assertIn("held back by allowance", creator[-1])

    def test_no_output_shows_a_venue_depth_or_names_a_venue(self):
        with FIXTURE.open() as file:
            depths = [int(r["venue_sol_reserve_before"]) for r in csv.DictReader(file)]
        for depth in depths:
            for shown in (str(depth), f"{depth / 10**9:.2f}", f"{depth / 10**9:g}"):
                whole_number = rf"(?<![\d.]){re.escape(shown)}(?![\d]|\.\d)"
                self.assertIsNone(re.search(whole_number, self.all_text), "a venue depth is shown")
        # The builder's creator check line names the launch curve (pump.fun's virtual tokens) as
        # the cause of the gap. It names no venue of a trade. No other text names a venue.
        others = re.sub(r"Real coin: .*\n", "", self.all_text)
        for venue_word in ("curve", "raydium", "pump.fun", "migration", "graduat"):
            self.assertNotIn(venue_word, others.lower())

    def test_no_clock_time_no_address_and_no_count_or_buy_total(self):
        self.assertIsNone(re.search(r"\b\d{1,2}:\d{2}\b", self.all_text))
        self.assertNotIn("fixture-wallet", self.all_text)
        self.assertNotIn("fixture-signature", self.all_text)
        for word in ("trades", "rows", "buys", "near in"):
            self.assertNotIn(word, builder_group_lines_removed(self.table).lower())

    def test_the_limits_off_label_has_no_brackets_inside_brackets(self):
        for name in ("replay-who-sold.md", "replay-summary.txt"):
            self.assertIn("no Fair Chance rule: no early fee and no sell limit", self.texts[name], name)
        for name, text in self.texts.items():
            self.assertIsNone(re.search(r"\([^()]*\(", visible_text(name, text)), name)

    def test_the_table_says_filled_none_and_filled_in_part(self):
        engine = self.rows()[1][-1]
        self.assertIn("filled none", engine)
        self.assertIn("filled in part", engine)
        self.assertNotIn(" fill;", self.table)
        self.assertNotRegex(self.table, r"(none|partial) fill")

    def test_every_published_output_passes_the_guard_at_one_decimal(self):
        for name, text in self.texts.items():
            self.assertEqual(findings_in_text(name, visible_text(name, text)), [], name)


class CreatorLimitsOnCellTest(unittest.TestCase):
    """The creator's limits-on cell has its own fallback; the summary and README repeat its form."""

    @classmethod
    def setUpClass(cls):
        cls.limits_off, cls.limits_on, cls.creator = fixture_runs()

    def at(self, form):
        return render(self.limits_off, self.limits_on, Precision(1, of_run_totals=True, creator=form), self.creator)

    def creator_row(self, texts):
        line = next(l for l in texts["replay-who-sold.md"].splitlines() if l.startswith("| creator |"))
        return [c.strip() for c in line.strip().strip("|").split("|")]

    def test_the_limits_on_cells_show_two_or_three_decimals_and_the_rest_one(self):
        for form, pattern in ((TWO_DECIMALS, r"^\d+\.\d\d%$"), (THREE_DECIMALS, r"^\d+\.\d\d\d%$")):
            texts = self.at(form)
            row = self.creator_row(texts)
            self.assertRegex(row[1], r"^\d+\.\d%$")
            self.assertRegex(row[3], pattern)
            self.assertRegex(row[4], pattern)
            # The summary and the README repeat the cells in the same precision.
            for name in ("replay-summary.txt", "readme-section.md"):
                self.assertIn(f"sold {row[3]} of all tokens sold in that run and took {row[4]}", texts[name])

    def test_the_ratio_sentence_compares_the_creator_payout_of_the_two_runs(self):
        def creator_payout(run):
            return sum(s.near for s in run.steps if s.group == "creator" and s.side == "sell")

        ratio = 100 * creator_payout(self.limits_on) / creator_payout(self.limits_off)
        sentence = f"With limits on, the creator took out {ratio:.2f}% of what it took with limits off."
        texts = self.at(RATIO)
        row = self.creator_row(texts)
        self.assertNotRegex(row[3] + row[4], r"\d")
        self.assertIn(sentence, texts["replay-who-sold.md"])
        for name in ("replay-summary.txt", "readme-section.md"):
            self.assertIn(f"the creator took out {ratio:.2f}% of what it took with limits off", texts[name])

    def test_the_counts_sentence_gives_the_creators_fills(self):
        # In the fixture the limits-on creator sells twice: one fills in part, one fills none.
        sentence = "With limits on, of the creator's 2 sells, 1 filled in part and 1 filled none."
        texts = self.at(COUNTS)
        self.assertNotRegex("".join(self.creator_row(texts)[3:5]), r"\d")
        self.assertIn(sentence, texts["replay-who-sold.md"])
        for name in ("replay-summary.txt", "readme-section.md"):
            self.assertIn("of the creator's 2 sells, 1 filled in part and 1 filled none", texts[name])

    def test_the_last_step_leaves_the_cell_out_and_says_why(self):
        texts = self.at(NOT_SHOWN)
        self.assertNotRegex("".join(self.creator_row(texts)[3:5]), r"\d")
        for name in ("replay-who-sold.md", "replay-summary.txt", "readme-section.md"):
            self.assertIn(NOT_SHOWN_LINE, texts[name])
        self.assertNotRegex(re.sub(r"Real coin: .*\n", "", texts["readme-section.md"]).split("| SMDD replay |")[1].split("|")[0], r"limits on.*\d")


GROUP = "funded directly by the same wallet before launch"


class SameFunderGroupTest(unittest.TestCase):
    """The group of wallets funded directly by one wallet before launch: its cells come from the
    run, no text says it sold, and the counts line shows only when it sold nothing."""

    @classmethod
    def setUpClass(cls):
        cls.limits_off, cls.limits_on, cls.creator = fixture_runs()
        cls.texts = render(cls.limits_off, cls.limits_on, ONE_DECIMAL, cls.creator)
        cls.table = cls.texts["replay-who-sold.md"]

    def test_the_label_names_direct_funding(self):
        with FIXTURE.open(newline="") as file:
            funded = {r["wallet"] for r in csv.DictReader(file) if r["role"].startswith("same-funder-")}
        self.assertEqual({s.group for s in self.limits_off.steps if s.account.startswith("same-funder-")}, {GROUP})
        self.assertEqual(len(funded), 2)

    def test_its_sell_cells_are_the_runs_true_value_or_not_shown(self):
        # In the fixture the group only buys, so the run's true sell share is zero in both runs.
        # Its cells are hidden here, so that the hidden 1% group cell cannot come back by
        # subtraction (SmallGroupTest).
        row = next(r for r in ReplayReportTest.rows(self) if r[0] == GROUP)
        self.assertTrue(all(cell in ("0%", "Not shown") for cell in row[1:5]), row)
        sold = [s for run in (self.limits_off, self.limits_on) for s in run.steps if s.group == GROUP and s.side == "sell"]
        self.assertEqual(sold, [])

    def test_no_text_says_the_group_sold(self):
        for name, text in self.texts.items():
            for line in visible_text(name, text).splitlines():
                if GROUP in line and not line.startswith("|"):
                    self.assertNotRegex(line.replace("sold none", ""), r"\b(sold|sells|sell|selling)\b", name)
            self.assertNotIn("insider", text.lower(), name)

    def test_the_readme_says_the_check_sees_direct_funding_only(self):
        self.assertIn(
            "The same-funder check sees direct funding only, so wallets funded through other wallets "
            "can sit in \"other\".",
            self.texts["readme-section.md"],
        )

    def test_the_counts_line_gives_the_group_count_and_its_share_of_buys(self):
        buys = [s for s in self.limits_off.steps if s.side == "buy"]
        group_paid = sum(s.near for s in buys if s.group == GROUP)
        share = f"{100 * group_paid / sum(s.near for s in buys):.1f}%"
        line = (
            f"2 wallets funded directly by one wallet before launch paid {share} of all NEAR paid in "
            "for buys in the first hour and sold none in the first hour."
        )
        self.assertEqual(same_funder_line(self.limits_off, self.limits_on, 1), line)

    def test_the_counts_line_is_absent_when_its_sell_cells_are_hidden(self):
        # In the fixture the group's sell cells hide the 1% group's cell from subtraction (SmallGroupTest).
        # "sold none" would give its 0% back, so the line is not written.
        self.assertEqual(table_rows(self.table)[GROUP][1:5], ["Not shown"] * 4)
        for name in ("replay-who-sold.md", "readme-section.md"):
            self.assertNotIn("sold none", self.texts[name], name)

    def test_the_counts_line_is_absent_when_the_group_sold(self):
        def step(side):
            return Step(account="same-funder-1", group=GROUP, minute=5, side=side, near=10, tokens=10,
                        fee_near=0, price_after=1.0)

        sold = Run([step("buy"), step("sell")], total_supply=100)
        self.assertIsNone(same_funder_line(sold, sold, 1))


def step(account, group, side, near=10, tokens=10, fill=None):
    return Step(account=account, group=group, minute=5, side=side, near=near, tokens=tokens, fee_near=0,
                price_after=1.0, fill=fill)


def table_rows(text):
    lines = [l for l in text.splitlines() if l.startswith("|") and "---" not in l]
    return {r[0]: r for r in ([c.strip() for c in l.strip().strip("|").split("|")] for l in lines)}


class SmallGroupTest(unittest.TestCase):
    """A group's cell shows only when the group has at least MIN_GROUP_SIZE wallets and the figure
    comes from at least MIN_GROUP_SIZE sells; else it says "Not shown". The creator is a named role
    and always shows."""

    @classmethod
    def setUpClass(cls):
        cls.limits_off, cls.limits_on, cls.creator = fixture_runs()
        cls.texts = render(cls.limits_off, cls.limits_on, ONE_DECIMAL, cls.creator)

    def test_the_least_group_size_is_two(self):
        self.assertEqual(MIN_GROUP_SIZE, 2)

    def test_a_group_of_one_wallet_shows_not_shown_in_every_cell(self):
        # In the fixture the 1% group is one wallet with one sell.
        row = table_rows(self.texts["replay-who-sold.md"])["bought 1% of supply or more in the first hour"]
        self.assertEqual(row[1:], ["Not shown"] * 5)
        self.assertIn(GROUP_NOT_SHOWN_LINE, self.texts["replay-who-sold.md"])
        self.assertEqual(
            GROUP_NOT_SHOWN_LINE,
            "Not shown: a group of fewer than 2 wallets, a share from fewer than 2 sells, or a share that "
            "would give another hidden figure back.",
        )

    def test_groups_of_two_or_more_wallets_and_sells_show(self):
        rows = table_rows(self.texts["replay-who-sold.md"])
        for group in ("creator", "other"):
            self.assertRegex(rows[group][1], r"^\d+\.\d%$", group)
            self.assertNotIn("Not shown", rows[group][-1], group)

    def test_a_share_from_one_sell_is_not_shown(self):
        other = "other"
        steps = [step("creator", "creator", "buy"), step("creator", "creator", "sell"),
                 step("creator", "creator", "sell"), step("other-1", other, "buy"), step("other-2", other, "buy"),
                 step("other-1", other, "sell")]
        run = Run(steps, total_supply=1_000)
        row = table_rows(table(run, run, ONE_DECIMAL))[other]
        self.assertEqual(row[1:5], ["Not shown"] * 4)
        two = Run(steps + [step("other-2", other, "sell")], total_supply=1_000)
        self.assertRegex(table_rows(table(two, two, ONE_DECIMAL))[other][1], r"^\d+\.\d%$")

    def test_a_sell_that_filled_none_does_not_count(self):
        steps = [step("creator", "creator", "sell"), step("creator", "creator", "sell"),
                 step("other-1", "other", "sell"), step("other-2", "other", "sell", near=0, tokens=0, fill="none")]
        run = Run(steps, total_supply=1_000)
        self.assertEqual(table_rows(table(run, run, ONE_DECIMAL))["other"][1:5], ["Not shown"] * 4)

    def test_no_hidden_cell_comes_back_as_one_hundred_minus_the_shown_cells(self):
        # Each run's shares of its own totals add up to 100%, so one hidden cell in a column could be
        # found by subtraction. A column never holds exactly one hidden group cell.
        for precision in (ONE_DECIMAL, Precision(1, of_run_totals=False, creator=TWO_DECIMALS)):
            rows = table_rows(render(self.limits_off, self.limits_on, precision, self.creator)["replay-who-sold.md"])
            body = [r for name, r in rows.items() if name != "Group"]
            for column in range(1, len(body[0]) - 1):
                hidden = [r[0] for r in body if r[column] == "Not shown"]
                self.assertNotEqual(len(hidden), 1, (precision, column))
                self.assertNotIn("creator", hidden)

    def test_the_second_hidden_cell_is_the_smallest_other_group_cell(self):
        # In the fixture the 1% group is hidden; the smallest other group cell is the same-funder 0%.
        rows = table_rows(self.texts["replay-who-sold.md"])
        self.assertEqual(rows[GROUP][1:5], ["Not shown"] * 4)
        self.assertRegex(rows["other"][1], r"^\d+\.\d%$")
        self.assertRegex(rows["creator"][1], r"^\d+\.\d%$")

    def test_a_hidden_creator_cell_does_not_come_back_by_subtraction(self):
        # Every group shows, and the creator's limits-on cells are not shown (the last creator form).
        # 100 minus the shown cells would give them back, so one more group cell is hidden there.
        one_percent = "bought 1% of supply or more in the first hour"
        steps = [step("creator", "creator", "sell"), step("creator", "creator", "sell")]
        for account, group, tokens in (("same-funder-1", GROUP, 5), ("same-funder-2", GROUP, 5),
                                       ("bought-one-percent-1", one_percent, 20), ("bought-one-percent-2", one_percent, 20),
                                       ("other-1", "other", 30), ("other-2", "other", 30)):
            steps += [step(account, group, "sell", near=tokens, tokens=tokens)] * 2
        run = Run(steps, total_supply=1_000)
        rows = table_rows(table(run, run, Precision(1, of_run_totals=True, creator=NOT_SHOWN)))
        body = [r for name, r in rows.items() if name != "Group"]
        self.assertEqual(rows["creator"][3:5], ["Not shown"] * 2)
        self.assertEqual(rows[GROUP][1:5], ["8.3%", "8.3%", "Not shown", "Not shown"])
        for column in range(1, 5):
            self.assertNotEqual(sum(r[column] == "Not shown" for r in body), 1, column)

    def test_a_column_with_two_hidden_cells_hides_no_more(self):
        steps = [step("creator", "creator", "sell"), step("creator", "creator", "sell"),
                 step("other-1", "other", "sell"), step("other-2", "other", "sell"),
                 step("same-funder-1", GROUP, "sell"),
                 step("bought-one-percent-1", "bought 1% of supply or more in the first hour", "sell")]
        run = Run(steps, total_supply=1_000)
        rows = table_rows(table(run, run, ONE_DECIMAL))
        self.assertRegex(rows["other"][1], r"^\d+\.\d%$")
        self.assertEqual(rows[GROUP][1], "Not shown")

    def test_the_counts_line_needs_two_wallets(self):
        one = Run([step("same-funder-1", GROUP, "buy"), step("same-funder-1", GROUP, "buy"),
                   step("other-1", "other", "buy")], total_supply=1_000)
        self.assertIsNone(same_funder_line(one, one, 1))
        two = Run([step("same-funder-1", GROUP, "buy"), step("same-funder-2", GROUP, "buy"),
                   step("other-1", "other", "buy")], total_supply=1_000)
        self.assertIsNotNone(same_funder_line(two, two, 1))


class FillCountsSentenceTest(unittest.TestCase):
    def test_every_group_is_named_when_none_is_zero(self):
        self.assertEqual(
            fill_counts_sentence(sells=6, full=3, part=2, none=1),
            "With limits on, of the creator's 6 sells, 3 filled in full, 2 filled in part and 1 filled none.",
        )

    def test_a_zero_group_is_not_named(self):
        self.assertEqual(
            fill_counts_sentence(sells=3, full=3, part=0, none=0),
            "With limits on, of the creator's 3 sells, 3 filled in full.",
        )
        self.assertEqual(
            fill_counts_sentence(sells=4, full=0, part=1, none=3),
            "With limits on, of the creator's 4 sells, 1 filled in part and 3 filled none.",
        )


class ShownShareTest(unittest.TestCase):
    def test_a_nonzero_share_that_rounds_to_zero_shows_below_the_last_digit(self):
        self.assertEqual(shown_share(4, 10_000, decimals=1), "below 0.1%")
        self.assertEqual(shown_share(4, 1_000, decimals=0), "below 1%")

    def test_a_true_zero_shows_zero_percent(self):
        self.assertEqual(shown_share(0, 10_000, decimals=1), "0%")
        self.assertEqual(shown_share(0, 10_000, decimals=0), "0%")
        self.assertEqual(shown_share(0, 0, decimals=1), "0%")

    def test_other_shares_show_at_their_precision(self):
        self.assertEqual(shown_share(1_234, 10_000, decimals=1), "12.3%")
        self.assertEqual(shown_share(1_234, 10_000, decimals=0), "12%")


class PrecisionFallbackTest(unittest.TestCase):
    def test_the_first_level_that_passes_is_taken(self):
        tried = []

        def findings_for(texts):
            tried.append(texts["level"])
            return [] if texts["level"] == 2 else ["table: row other, column x"]

        choice = first_passing(PRECISION_LEVELS, lambda p: {"level": PRECISION_LEVELS.index(p)}, findings_for)
        self.assertEqual(choice.precision, PRECISION_LEVELS[2])
        self.assertEqual(choice.precision, Precision(decimals=1, of_run_totals=True, creator=RATIO))
        self.assertEqual(tried, [0, 1, 2])

    def test_the_creator_cell_falls_back_two_then_three_decimals_then_ratio_counts_and_not_shown(self):
        self.assertEqual(CREATOR_FORMS, [TWO_DECIMALS, THREE_DECIMALS, RATIO, COUNTS, NOT_SHOWN])

    def test_no_level_shows_whole_percents(self):
        self.assertEqual(
            PRECISION_LEVELS,
            [Precision(1, of_run_totals=True, creator=f) for f in CREATOR_FORMS]
            + [Precision(1, of_run_totals=False, creator=f) for f in CREATOR_FORMS],
        )

    def test_when_no_level_passes_nothing_is_chosen_and_the_cells_are_named(self):
        choice = first_passing(
            PRECISION_LEVELS, lambda p: {"x": ""}, lambda texts: ["table: row creator, column y"]
        )
        self.assertIsNone(choice.precision)
        self.assertEqual(choice.texts, {})
        self.assertEqual(choice.findings, ["table: row creator, column y"])


if __name__ == "__main__":
    unittest.main()
