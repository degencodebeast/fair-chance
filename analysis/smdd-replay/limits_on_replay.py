"""Plays the trades with every limit on: the same coin code, in the near-sdk in-process runner,
with exact blocks and times.

The trade sizes come from `limits_off_replay` (one sizing for both runs). A runner is any object
with `rules()` (the coin's `fc_rules` and `ft_total_supply`) and `play(plan)` (one result per
trade); `InProcessCoinRunner` runs `contracts/coin/examples/replay-trades-in-process.rs`.
"""
from __future__ import annotations

import json
import re
import subprocess
import tempfile
from pathlib import Path

from limits_off_replay import Run, SellSizer, Step, blocks_after_launch
from trade_file import Trade

REPO = Path(__file__).resolve().parents[2]
EXAMPLE = "replay-trades-in-process"


class InProcessCoinRunner:
    """The coin contract in the near-sdk in-process runner (cargo example, built on first use)."""

    def __init__(self, repo: Path = REPO) -> None:
        self.repo = repo
        self.binary: Path | None = None

    def _run(self, *args: str) -> dict:
        if self.binary is None:
            subprocess.run(
                ["cargo", "build", "--quiet", "--locked", "-p", "fair-chance-coin", "--example", EXAMPLE],
                cwd=self.repo,
                check=True,
            )
            self.binary = self.repo / "target" / "debug" / "examples" / EXAMPLE
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / "out.json"
            inputs = []
            for index, value in enumerate(args):
                if isinstance(value, dict):
                    path = Path(folder) / f"in-{index}.json"
                    path.write_text(json.dumps(value))
                    value = str(path)
                inputs.append(value)
            # The runner prints the mock chain's log lines; show them only when the run fails.
            done = subprocess.run([str(self.binary), *inputs, str(out)], capture_output=True, text=True)
            if done.returncode != 0:
                raise RuntimeError(f"{EXAMPLE} failed:\n{done.stderr[-2000:]}")
            return json.loads(out.read_text())

    def rules(self) -> tuple[dict, int]:
        answer = self._run("rules")
        return answer["fc_rules"], int(answer["total_supply"])

    def play(self, plan: dict) -> list[dict]:
        return self._run("play", plan)["results"]


class SandboxCoinRunner:
    """The coin on a fresh sandbox (`sandbox-tests/src/bin/replay-trades-on-sandbox.rs`). A real
    chain picks its own blocks, so it plays the plan after the early window only, and it answers
    in one run: `fc_rules`, `total_supply`, `start_pool` and the results. Needs
    NEAR_SANDBOX_BIN_PATH; it is a heavy run."""

    BINARY = "replay-trades-on-sandbox"

    def __init__(self, repo: Path = REPO) -> None:
        self.repo = repo

    def play_all(self, plan: dict) -> dict:
        # The script builds the coin and factory wasm first: the sandbox refuses a stale wasm.
        script = self.repo / "scripts" / f"{self.BINARY}.sh"
        with tempfile.TemporaryDirectory() as folder:
            plan_file, out = Path(folder) / "plan.json", Path(folder) / "out.json"
            plan_file.write_text(json.dumps(plan))
            done = subprocess.run(
                [str(script), "play", str(plan_file), str(out)], cwd=self.repo, capture_output=True, text=True
            )
            if done.returncode != 0:
                raise RuntimeError(f"{self.BINARY} failed:\n{done.stderr[-2000:]}")
            return json.loads(out.read_text())


def plan_for(trades: list[Trade]) -> dict:
    """The runner's plan: each buy as its share of its venue's SOL depth (the runner takes that
    share of the pool's live NEAR depth, as `near_for_buy` does), each sell as a share of the
    account's live balance. The plan file is temporary; it holds venue depths."""
    file_balances = SellSizer()
    planned = []
    for trade in trades:
        entry = {
            "account": trade.account,
            "blocks_after_launch": blocks_after_launch(trade.seconds_from_launch),
            "seconds_from_launch": trade.seconds_from_launch,
            "side": trade.side,
        }
        if trade.side == "buy":
            entry["buy_sol_lamports"] = str(trade.sol_lamports)
            entry["buy_venue_sol_depth"] = str(trade.venue_sol_reserve_before)
            file_balances.bought(trade, 0)
        else:
            before = file_balances.file_balance.get(trade.account, 0)
            entry["sell_numerator"] = str(trade.token_base_units if before else 0)
            entry["sell_denominator"] = str(before or 1)
            file_balances.sold(trade, 0)
        planned.append(entry)
    return {"trades": planned}


def _refusal_code(text: str | None) -> str | None:
    if text is None:
        return None
    found = re.search(r"FC_[A-Z_]+", text)
    return found.group(0) if found else "refused"


def play_limits_on(trades: list[Trade], runner) -> Run:
    rules, total_supply = runner.rules()
    results = runner.play(plan_for(trades))
    steps = []
    for trade, result in zip(trades, results, strict=True):
        steps.append(
            Step(
                account=trade.account,
                group=trade.group,
                minute=trade.seconds_from_launch // 60,
                side=trade.side,
                near=int(result["near"]),
                tokens=int(result["tokens"]),
                fee_near=int(result.get("fee_near") or 0),
                price_after=int(result["near_reserve"]) / int(result["token_reserve"]),
                tokens_asked=int(result["tokens_asked"]) if result.get("tokens_asked") else None,
                holder_class=result.get("class"),
                fill=result.get("fill"),
                limit_reason=result.get("limit_reason"),
                refusal=_refusal_code(result.get("refusal")),
            )
        )
    # A fresh coin's pool: virtual NEAR against the whole supply.
    return Run(steps, total_supply, int(rules["virtual_near"]) / total_supply)
