SMDD first hour: replay with the limits off and on.

| Group | Tokens sold, limits off (share of all tokens sold in that run) | NEAR paid out, limits off (share of all NEAR paid out in that run) | Tokens sold, limits on (share of all tokens sold in that run) | NEAR paid out, limits on (share of all NEAR paid out in that run) | What the rule engine did, limits on |
| --- | --- | --- | --- | --- | --- |
| creator | 4.7% | 10.2% | 0.02% | 0.02% | class large; filled in part; filled none; held back by cap_full; held back by floor |
| funded directly by the same wallet before launch | 0% | 0% | 0% | 0% | class band, large |
| bought 1% of supply or more in the first hour | 75.2% | 62.8% | 76.8% | 79.8% | class small, band, large; filled in part; filled none; held back by cap_full; refused FC_EARLY_WINDOW |
| other | 20.1% | 27.1% | 23.2% | 20.1% | class small; filled in part; filled none; held back by allowance; held back by cap_full; refused FC_EARLY_WINDOW |

5 wallets funded directly by one wallet before launch paid 0.5% of all NEAR paid in for buys in the first hour and sold none in the first hour.

Each share is a share of that run's own total: all tokens sold, or all NEAR paid out to sellers, with one total for the limits-off run, another for the limits-on run. The two runs have different totals, so the shares are not amounts and the columns of the two runs do not compare as tokens or NEAR.

Limits off: no Fair Chance rule: no early fee and no sell limit, the same pool math with its normal swap fee, in the script. Limits on: all rules together (early fee, allowance, class and cap), in the same coin code, in the near-sdk in-process runner, with exact blocks and times. The difference between the two runs comes from all these rules together, not from one of them.

How the replay sizes a trade: A buy takes the same share of the Fair Chance pool's NEAR depth just before it as it took of its own venue's SOL depth just before the trade. A sell sells the same share of the account's replay balance as it sold of its own balance in the trade file. Blocks: 0.5789 seconds per block: NEAR mainnet, measured from the block heights and timestamps in the committed transaction files of analysis/near-replay/fixtures (an interim value until a testnet measure replaces it).
