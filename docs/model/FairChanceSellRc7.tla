-------------------------- MODULE FairChanceSellRc7 --------------------------
(* One sell on a Fair Chance coin, from the docs at INTERFACE 1.0.0-rc.7.    *)
(* Cross-checked against rule-engine/src/actions.rs, real_near_cap.rs,       *)
(* allowance.rs and holder_class.rs at the same version.                     *)
(* Abstractions: a sell pays a fixed 2 NEAR per token; a buy pays 1 or 2     *)
(* NEAR per token (1 stands for a buy made when the price was lower, so a    *)
(* later sell makes a profit); no swap fee, no creator share; a fixed token- *)
(* reserve snapshot (allowance and floor are constants); no sends; no early *)
(* window; one early-supply account M (its pro-rata share is the whole      *)
(* shared allowance and the whole shared floor). Penalty: 1 of the 2 NEAR   *)
(* per token stays in the pool. The NEAR reserve minimum never binds: a sell*)
(* is asked only when 2 x tokens asked <= real NEAR.                        *)
EXTENDS Integers, FiniteSets

CONSTANTS
  A,          \* unmarked allowance per 1 h window, tokens
  MA,         \* shared early-supply allowance per long window, tokens
  F,          \* floor, tokens (10 bps of the window snapshot, rounded down)
  T,          \* large threshold, tokens
  CNum, CDen, \* cap share c = CNum / CDen
  MaxBal, MaxWin, MaxActs, LongWin

Accts == {"L", "S", "M"}
SMALL == 0  BAND == 1  LARGE == 2
Limited == {BAND, LARGE}

VARIABLES
  unm, mk, wasLarge, real,              \* balances, is_large flag, real NEAR
  win, acts, phase, pend,
  touched, startCls, fBand, largeInWin, maxBalWin, \* 1 h window class state
  soldU, boughtU, soldMk,               \* tokens in the 1 h window
  mOut, net, bns,                       \* cap record per account (1 h window)
  cheap,                                \* tokens bought at 1 NEAR in the window
  floorTaken,                           \* tokens filled past the cap this window
  mUsed                                 \* marked sold in the long window

vars == <<unm, mk, wasLarge, real, win, acts, phase, pend, touched, startCls,
          fBand, largeInWin, maxBalWin, soldU, boughtU, soldMk, mOut, net, bns,
          cheap, floorTaken, mUsed>>

Max2(x, y) == IF x >= y THEN x ELSE y
Min2(x, y) == IF x <= y THEN x ELSE y
Pos(x) == Max2(x, 0)
MaxOf(S) == CHOOSE x \in S : \A y \in S : y <= x

Bal(a) == unm[a] + mk[a]
\* holder_class.rs live_class: is_large first, else band above one allowance.
LiveCls(b, wl) == IF b >= T \/ (wl /\ b > 0) THEN LARGE
                  ELSE IF b > A THEN BAND ELSE SMALL
\* INTERFACE l21: live class raised to the class floor, band at most from the floor.
ClsOf(a, b, wl) == LET l == LiveCls(b, wl) IN
                   IF l = SMALL /\ touched[a] /\ fBand[a] THEN BAND ELSE l
Cls(a) == ClsOf(a, Bal(a), wasLarge[a])

\* INTERFACE l59 cap_used, ADR 0005 l13: marked NEAR out gross, plus the net
\* clamped at zero when read.
CapValOf(mo, n) == mo + Pos(n)
CapVal(a) == CapValOf(mOut[a], net[a])
RECURSIVE SumOver(_, _)
SumOver(f, S) == IF S = {} THEN 0 ELSE LET x == CHOOSE y \in S : TRUE
                                       IN f[x] + SumOver(f, S \ {x})
CapUsedWith(a, mo, n) ==
  CapValOf(mo, n) + SumOver([x \in Accts \ {a} |-> CapVal(x)], Accts \ {a})
CapUsed == SumOver([x \in Accts |-> CapVal(x)], Accts)

Init ==
  /\ unm = [a \in Accts |-> CASE a = "L" -> 4 [] a = "S" -> 1 [] OTHER -> 0]
  /\ mk = [a \in Accts |-> IF a = "M" THEN 2 ELSE 0]
  /\ wasLarge = [a \in Accts |-> a = "L"]
  /\ real = 2 * 7
  /\ win = 0 /\ acts = 0 /\ phase = "ready" /\ pend = [acct |-> "none"]
  /\ touched = [a \in Accts |-> FALSE]
  /\ startCls = [a \in Accts |-> SMALL]
  /\ fBand = [a \in Accts |-> FALSE]
  /\ largeInWin = [a \in Accts |-> FALSE]
  /\ maxBalWin = [a \in Accts |-> 0]
  /\ soldU = [a \in Accts |-> 0] /\ boughtU = [a \in Accts |-> 0]
  /\ soldMk = [a \in Accts |-> 0]
  /\ mOut = [a \in Accts |-> 0] /\ net = [a \in Accts |-> 0]
  /\ bns = [a \in Accts |-> 0] /\ cheap = [a \in Accts |-> 0]
  /\ floorTaken = [a \in Accts |-> 0]
  /\ mUsed = 0

\* holder_class.rs raise_class_floor, called before and after each action.
TouchTo(a, bAfter, wlAfter) ==
  LET lb == LiveCls(Bal(a), wasLarge[a])
      la == LiveCls(bAfter, wlAfter) IN
  /\ touched' = [touched EXCEPT ![a] = TRUE]
  /\ startCls' = [startCls EXCEPT ![a] = IF touched[a] THEN @ ELSE lb]
  /\ fBand' = [fBand EXCEPT ![a] =
                 (touched[a] /\ @) \/ lb /= SMALL \/ la /= SMALL]
  /\ largeInWin' = [largeInWin EXCEPT ![a] =
                 (touched[a] /\ @) \/ lb = LARGE \/ la = LARGE]
  /\ maxBalWin' = [maxBalWin EXCEPT ![a] =
                 Max2(IF touched[a] THEN @ ELSE Bal(a), Max2(Bal(a), bAfter))]

\* A buy of n tokens at p NEAR each. actions.rs buy: count_bought, cap net -=
\* NEAR paid, bought_not_sold += unmarked tokens.
Buy(a, n, p) ==
  /\ phase = "ready" /\ acts < MaxActs /\ Bal(a) + n <= MaxBal
  /\ TouchTo(a, Bal(a) + n, wasLarge[a] \/ Bal(a) + n >= T)
  /\ unm' = [unm EXCEPT ![a] = @ + n]
  /\ wasLarge' = [wasLarge EXCEPT ![a] = @ \/ Bal(a) + n >= T]
  /\ real' = real + p * n
  /\ boughtU' = [boughtU EXCEPT ![a] = @ + n]
  /\ net' = [net EXCEPT ![a] = @ - p * n]
  /\ bns' = [bns EXCEPT ![a] = @ + n]
  /\ cheap' = [cheap EXCEPT ![a] = @ + (IF p = 1 THEN n ELSE 0)]
  /\ acts' = acts + 1
  /\ UNCHANGED <<mk, win, phase, pend, soldU, soldMk, mOut, floorTaken, mUsed>>

\* The sell decision (actions.rs SellFillPlan, sell_outcome), stored as a receipt.
Quote(a, k) ==
  LET cls    == Cls(a)
      mLim   == Min2(Min2(k, mk[a]), Pos(MA - mUsed))   \* marked part, up to the share left
      \* INTERFACE l64, ADR 0005 l17 (rc.7): marked floor left = its part of the shared
      \* floor - marked sold in the 24 h window - unmarked sold in the 1 h window
      mFL    == Pos(F - mUsed - soldU[a])
      mAlw   == Min2(mLim, mFL)
      uk     == Min2(k - mLim, unm[a])
      room   == Pos(A + boughtU[a] - soldU[a])           \* INTERFACE l62: a buy gives room back
      first  == Pos(A - soldU[a])                        \* band uncounted part, no room back
      \* INTERFACE l63, ADR 0005 l17 (rc.7): floor left = floor - every token sold in
      \* the 1 h window, marked and unmarked; a buy gives no floor room back
      fLU    == Pos(F - soldU[a] - soldMk[a])
      b0     == bns[a]
      most   == IF cls = LARGE THEN Min2(uk, room) ELSE uk
      pf     == IF cls = BAND THEN room ELSE uk
      nc     == CASE cls = LARGE -> 0 [] cls = BAND -> first [] OTHER -> uk
      UAlw(m, fl) == CASE cls = LARGE -> IF fl THEN Min2(most, Pos(fLU - m)) ELSE 0
                       [] cls = BAND  -> Min2(uk, room)
                       [] OTHER       -> IF b0 = 0 THEN uk ELSE 0
      Pay(m, u)    == 2 * m + 2 * Min2(u, pf) + Pos(u - pf)
      Unc(u)       == Min2(u - Min2(u, b0), nc)
      NetAdd(m, u) == Pay(m, u) - 2 * m - 2 * Unc(u)
      UsedW(m, u)  == CapUsedWith(a, mOut[a] + 2 * m, net[a] + NetAdd(m, u))
      Ok(m, u)     == CDen * UsedW(m, u) <= CNum * (real - Pay(m, u))
      FillM(fl)    == LET lo == IF fl THEN mAlw ELSE 0 IN
                      MaxOf({lo} \cup {x \in lo..mLim : Ok(x, UAlw(x, fl))})
      \* A small part that adds nothing to the cap used may fill without the cap check,
      \* but not next to a marked part: the marked part's own payout must pass the
      \* check (ADR 0005 l12, l15; INTERFACE l9 "may fill less of the bought part").
      FillU(m, fl) == LET lo == UAlw(m, fl) IN
                      MaxOf({lo} \cup {x \in lo..most :
                         Ok(m, x) \/ (cls = SMALL /\ UsedW(m, x) <= UsedW(m, 0)
                                      /\ ~(m > 0 /\ x > 0))})
      mF     == FillM(TRUE)
      uF     == FillU(mF, TRUE)
      m0     == FillM(FALSE)
      u0     == FillU(m0, FALSE)
      filled == mF + uF
      alwM   == UAlw(mF, TRUE)
      \* actions.rs limit_reason
      uAtFloor == cls = LARGE /\ uF = Pos(fLU - mF) /\ uF > 0
      reason == IF filled = k THEN "null"
                ELSE IF mF < mLim THEN (IF mF = mAlw /\ (mF > 0 \/ uAtFloor)
                                        THEN "floor" ELSE "cap_full")
                ELSE IF uF = most THEN "allowance"
                ELSE IF uAtFloor THEN "floor"
                ELSE "cap_full"
      \* INTERFACE l68 read as written: "floor when a large sell, or the marked
      \* part of a sell, fills only its floor (the part that fills at any cap)
      \* and the cap stops the rest"; rc.7 (l9): also a large sell with marked
      \* floor_left 0 that fills exactly its unmarked floor. The fill must equal
      \* the floor, not an ask or allowance below it.
      docReason ==
                IF filled = k THEN "null"
                ELSE IF filled = mLim + most THEN "allowance"
                ELSE IF filled > 0 /\
                        ((cls = LARGE /\ mF <= mAlw /\ uF = Pos(fLU - mF)) \/
                         (mF = mAlw /\ mF > 0 /\ mF < mLim))
                     THEN "floor"
                ELSE "cap_full"
  IN [acct |-> a, cls |-> cls, ask |-> k, filled |-> filled,
      mF |-> mF, uF |-> uF, mLim |-> mLim, mAlw |-> mAlw, uk |-> uk,
      room |-> room, most |-> most, alwM |-> alwM, fLU |-> fLU,
      pay |-> Pay(mF, uF), netAdd |-> NetAdd(mF, uF), unc |-> Unc(uF),
      bsSold |-> Min2(uF, b0), bnsBefore |-> b0, cheapBefore |-> cheap[a],
      penTok |-> Pos(uF - pf),
      capBefore |-> CapUsed, capAfter |-> UsedW(mF, uF),
      capOkAfter |-> Ok(mF, uF),
      capFullBefore |-> CDen * CapUsed >= CNum * real,
      okNext |-> Ok(mF, uF + 1), usedNext |-> UsedW(mF, uF + 1),
      floorUse |-> IF cls = LARGE \/ mLim > 0 THEN Pos(filled - (m0 + u0)) ELSE 0,
      reason |-> reason, docReason |-> docReason,
      markedBefore |-> mk[a], wasLargeBefore |-> wasLarge[a],
      largeInWin |-> (touched[a] /\ largeInWin[a]) \/ LiveCls(Bal(a), wasLarge[a]) = LARGE,
      maxBalWin |-> Max2(IF touched[a] THEN maxBalWin[a] ELSE Bal(a), Bal(a)),
      netAfter |-> Pos(soldU[a] + uF - boughtU[a]),
      fill |-> IF filled = k THEN "full" ELSE IF filled = 0 THEN "none" ELSE "partial"]

AskSell(a, k) ==
  /\ phase = "ready" /\ acts < MaxActs /\ k \in 1..Bal(a) /\ 2 * k <= real
  /\ phase' = "selling" /\ pend' = Quote(a, k)
  /\ UNCHANGED <<unm, mk, wasLarge, real, win, acts, touched, startCls, fBand,
                 largeInWin, maxBalWin, soldU, boughtU, soldMk, mOut, net, bns,
                 cheap, floorTaken, mUsed>>

Settle ==
  /\ phase = "selling"
  /\ LET p == pend  a == p.acct  b == Bal(a) - p.filled IN
     /\ TouchTo(a, b, wasLarge[a] /\ b > 0)
     /\ unm' = [unm EXCEPT ![a] = @ - p.uF]
     /\ mk' = [mk EXCEPT ![a] = @ - p.mF]
     /\ wasLarge' = [wasLarge EXCEPT ![a] = @ /\ b > 0]
     /\ real' = real - p.pay
     /\ soldU' = [soldU EXCEPT ![a] = @ + p.uF]
     /\ soldMk' = [soldMk EXCEPT ![a] = @ + p.mF]
     /\ mOut' = [mOut EXCEPT ![a] = @ + 2 * p.mF]
     /\ net' = [net EXCEPT ![a] = @ + p.netAdd]
     /\ bns' = [bns EXCEPT ![a] = @ - p.bsSold]
     /\ floorTaken' = [floorTaken EXCEPT ![a] = @ + p.floorUse]
     /\ mUsed' = mUsed + p.mF
  /\ phase' = "ready" /\ acts' = acts + 1
  /\ UNCHANGED <<win, pend, boughtU, cheap>>

NewWindow ==
  /\ phase = "ready" /\ win + 1 < MaxWin
  /\ win' = win + 1 /\ acts' = 0
  /\ touched' = [a \in Accts |-> FALSE]
  /\ startCls' = [a \in Accts |-> SMALL]
  /\ fBand' = [a \in Accts |-> FALSE]
  /\ largeInWin' = [a \in Accts |-> FALSE]
  /\ maxBalWin' = [a \in Accts |-> 0]
  /\ soldU' = [a \in Accts |-> 0] /\ boughtU' = [a \in Accts |-> 0]
  /\ soldMk' = [a \in Accts |-> 0]
  /\ mOut' = [a \in Accts |-> 0] /\ net' = [a \in Accts |-> 0]
  /\ bns' = [a \in Accts |-> 0] /\ cheap' = [a \in Accts |-> 0]
  /\ floorTaken' = [a \in Accts |-> 0]
  /\ mUsed' = IF (win + 1) % LongWin = 0 THEN 0 ELSE mUsed
  /\ pend' = [acct |-> "none"]
  /\ UNCHANGED <<unm, mk, wasLarge, real, phase>>

Done == phase = "ready" /\ win + 1 = MaxWin /\ UNCHANGED vars

Next ==
  \/ \E a \in Accts, n \in 1..MaxBal, p \in {1, 2} : Buy(a, n, p)
  \/ \E a \in Accts, k \in 1..MaxBal : AskSell(a, k)
  \/ Settle
  \/ NewWindow
  \/ Done

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Rules. Each cites the doc line it comes from (INTERFACE 1.0.0-rc.7).      *)
Selling == phase = "selling"
P == pend

\* INTERFACE l21, ADR 0001 l17: never below the class just before the first
\* action in the window, band at most from the floor.
ClassFloor == \A a \in Accts : touched[a] => Cls(a) >= Min2(startCls[a], BAND)
\* ADR 0001 l17, GLOSSARY l20: a large account at zero tokens is band for the
\* rest of the window.
LargeToZeroIsBand == \A a \in Accts :
  (touched[a] /\ largeInWin[a] /\ Bal(a) = 0) => Cls(a) = BAND
\* ADR 0001 l16, GLOSSARY l18, INTERFACE l21: stays large until zero.
LargeUntilZero == \A a \in Accts : (wasLarge[a] /\ Bal(a) > 0) => Cls(a) = LARGE
\* OVERVIEW l20: a bag inside one allowance, never large, never above one
\* allowance in the window, no early supply: one limit only. With no tokens
\* bought this hour, it fills in full and adds nothing to the cap.
BagInsideAllowanceNoLimit ==
  Selling /\ P.markedBefore = 0 /\ P.maxBalWin <= A /\ ~P.largeInWin
  => /\ P.cls = SMALL
     /\ (P.bnsBefore = 0 => (P.filled = P.ask /\ P.capAfter = P.capBefore))
\* ADR 0005 l15 (rc.7 keeps this for a sell with no marked part): small sells of tokens held from before the window never
\* count; a small sell of bought tokens counts the part above the NEAR paid and
\* fills only as far as the cap room allows; no floor. So a small sell stops
\* short only where one more token would both fail the cap check and add to
\* the cap used.
SmallAlwaysPasses == Selling /\ P.cls = SMALL /\ P.markedBefore = 0 =>
  /\ (P.bnsBefore = 0 => (P.filled = P.ask /\ P.capAfter = P.capBefore))
  /\ (P.filled < P.ask => (~P.okNext /\ P.usedNext > P.capBefore))
\* ADR 0004 l14, l18: a large unmarked part fills up to the net allowance.
LargeNetAllowance == Selling /\ P.cls = LARGE => P.netAfter <= A
\* ADR 0004 l18, INTERFACE l8, l28, l59, ADR 0005 l13: the marked allowance
\* counts gross, and marked NEAR out counts gross toward the cap: no buy ever
\* offsets it.
MarkedGross == /\ mUsed <= MA
               /\ \A a \in Accts : mOut[a] = 2 * soldMk[a] /\ CapVal(a) >= mOut[a]
\* ADR 0004 l16, ADR 0005 l16: a band first allowance always fills.
BandFirstAllowanceFills == Selling /\ P.cls = BAND => P.uF >= Min2(P.uk, P.room)
\* ADR 0005 l16: the band part above the first allowance fills zero when the
\* cap is full.
BandAboveZeroWhenFull == Selling /\ P.cls = BAND /\ P.capFullBefore => P.penTok = 0
\* ADR 0005 l12: every large, band and marked sell is checked after the
\* sell against c x real NEAR left, also when its own net adds zero, except
\* the parts that always fill (a large floor and a band first allowance).
CapCheckEverySell ==
  Selling /\ (P.cls \in Limited \/ P.mF > 0) /\ (P.mF > P.mAlw \/ P.uF > P.alwM)
  => P.capOkAfter
\* ADR 0005 l16-17: large and marked sells stop at the floor when the cap is full.
FloorMinimum == Selling =>
  /\ (P.cls = LARGE => P.uF >= P.alwM)
  /\ P.mF >= P.mAlw
\* Rule (ADR 0005 l17, INTERFACE l9, GLOSSARY l37): no
\* account takes more than one floor (marked plus unmarked together, in either
\* order) in one 1 h window. Floor taken = tokens a sell fills past what the
\* cap alone allows. One floor = the larger of F and the marked floor left at
\* the first sell; both are F here (one marked holder, same snapshot).
NoTwoFloorsInOneWindow == \A a \in Accts : floorTaken[a] <= F
\* Diagnostic, not a doc rule: the same, but each token bought in the window
\* may give one floor token back (the ADR 0005 l17 formula uses net used).
NoTwoFloorsNetOfBuys == \A a \in Accts : floorTaken[a] <= F + boughtU[a]
\* ADR 0005 l26: an unmarked round trip by one account (bought at the sell
\* price) adds nothing to the cap total.
RoundTripAddsNothing == \A a \in Accts :
  (soldMk[a] = 0 /\ cheap[a] = 0 /\ soldU[a] <= boughtU[a]) => CapVal(a) = 0
\* ADR 0005 l29, OVERVIEW l55: a band account that buys and sells back in the
\* same window counts the bought part toward the cap, net of its buy.
BandBuyBackCounts == Selling /\ P.cls = BAND /\ P.mF = 0 /\ P.uF > 0
  /\ P.uF <= P.bnsBefore => P.netAdd = P.pay
\* ADR 0004 l15, l18: only the part above the (net) allowance pays the penalty.
PenaltyOnlyAboveNetAllowance == Selling /\ P.cls = BAND
  => P.penTok <= Pos(P.netAfter - A)
\* INTERFACE l67: fill codes.
FillCodes == Selling => (P.fill = "full") = (P.filled = P.ask)
\* INTERFACE l68, l9: the limit reason the code gives matches the doc sentence.
LimitReasonMatchesDoc == Selling => P.reason = P.docReason

\* (ADR 0005 l15, INTERFACE l9, l60): a small sell with a marked part may
\* fill less of the bought part only when the cap is tight: one more token
\* fails the cap check.
SmallBoughtPartNextToMarked == Selling /\ P.cls = SMALL /\ P.mF > 0
  /\ P.mF = P.mLim /\ P.filled < P.ask /\ P.uF < P.uk => ~P.okNext

Bound == acts <= MaxActs
=============================================================================
