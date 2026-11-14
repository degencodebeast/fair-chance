# TLA+ model of one sell

`FairChanceSellRc7.tla` models one sell on a Fair Chance coin as the docs describe it at interface `1.0.0-rc.7`. TLC, the TLA+ model checker, tries every reachable state within the bounds below and checks each rule as an invariant.

## What it covers

- Three accounts: a large holder `L`, a small holder `S` and an early-supply holder `M`.
- Buys and sells, class and class floor, the unmarked allowance, the shared early-supply allowance, the floor (one per account per 1 hour window), the pool-wide real NEAR cap, band penalty, fill codes and `limit_reason`.
- Two 1 hour windows, so a window reset happens inside a run.

## What it leaves out

- Prices. A sell pays a fixed 2 NEAR per token, and a buy pays 1 or 2 NEAR per token.
- Swap fees, the creator share, the early window and early fee, sends, and the NEAR reserve minimum.
- The real code. The model follows the docs. `rule-engine/` is checked by its own tests.
- Sizes beyond the bounds. A rule that fails only at larger sizes would not show here.

## Rules checked

Nineteen invariants, each with the doc line it comes from in the model file: `ClassFloor`, `LargeToZeroIsBand`, `LargeUntilZero`, `BagInsideAllowanceNoLimit`, `SmallAlwaysPasses`, `LargeNetAllowance`, `MarkedGross`, `BandFirstAllowanceFills`, `BandAboveZeroWhenFull`, `CapCheckEverySell`, `FloorMinimum`, `NoTwoFloorsInOneWindow`, `NoTwoFloorsNetOfBuys`, `RoundTripAddsNothing`, `BandBuyBackCounts`, `PenaltyOnlyAboveNetAllowance`, `FillCodes`, `LimitReasonMatchesDoc`, `SmallBoughtPartNextToMarked`.

## Bounds

In `Rc7.cfg`: allowance 2 tokens, shared early allowance 2, floor 1, large threshold 4, cap share 1/4, at most 4 tokens per account, 2 windows, 3 actions per window.

## How to run

You need a recent Java (the last run used OpenJDK 27) and `tla2tools.jar`. The last run used TLC `2026.11.06.222357` (rev `d8fb231`), from a `tla2tools.jar` with sha256 `e1f11b75c4b5bc4478ba0ac5cb6ae28e40a01b0016bc33e0aa97da22dfdbcd7f` (check yours with `shasum -a 256 tla2tools.jar`).

```sh
cd docs/model
java -XX:+UseParallelGC -cp /path/to/tla2tools.jar tlc2.TLC -workers 1 -config Rc7.cfg FairChanceSellRc7.tla
```

## Result of the last run

```
Model checking completed. No error has been found.
1675393 states generated, 772733 distinct states found, 0 states left on queue.
The depth of the complete state graph search is 14.
Finished in 01min 06s
```

The exit code was 0. All 19 invariants held in every reachable state. The run took one worker on one Apple-silicon laptop.

## What changed during this check

An earlier run of the same model with a docs reading reported 29 violations of `CapCheckEverySell`. All came from one path. An account sold part of its early supply, bought tokens, and then sold the bought part together with more early supply. The model let the bought part fill with no cap check, because it adds nothing to the cap used. The docs say otherwise: every sell with a marked part is checked after the sell (ADR 0005), and a bought part next to a marked part "may fill less" (INTERFACE `1.0.0-rc.7`). The code does that. So the model was wrong, not the docs. The model now skips the cap check for a bought part only when the sell has no marked part. No invariant was removed or weakened.

The model once had a second config, `Rc7-code.cfg`, with a flag that copied a rounding effect of the Rust code. With this fix the two configs checked the same rules, so one config remains.
