// The words on the landing screen. The headline is the builder's. The rest follows the v5 design,
// with "first" replaced where the copy rules ban it. Rules claims match docs/OVERVIEW.md.
export type LandingAction = { label: string; opens: "coins" | "launch-off" | "rules" };

export const landingCopy = {
  headline: "You buy. The creator dumps on you. You hold the bag.",
  subheadline: "On Fair Chance, they have to sell slowly.",
  intro:
    "Every coin launched here has sell limits built into its contract. The creator, the snipers and the big wallets can still sell. They just can't sell it all at once.",
  actions: [
    { label: "Open app", opens: "coins" },
    { label: "Launch a coin", opens: "launch-off" },
    { label: "See the rules on a live coin", opens: "rules" },
  ] satisfies LandingAction[],
  launchOffNote: "Launching signs a transaction. This page does not sign yet.",
  proof: {
    label: "Sample data. An example receipt, not a trade on chain.",
    sentence: "A big holder tried to sell 3% of a coin. 0.7% went through. It can sell up to 0.7% more after 15:02.",
  },
  problem:
    "On most launchpads, the creator and the people who buy in the opening seconds often sell into you within minutes. A warning tool can tell you it happened. It can't stop it.",
  buyers: {
    kicker: "For buyers",
    title: "The same rules on every coin. Read them before you buy.",
    items: [
      { b: "The creator and the snipers can't sell at launch.", t: "Anyone who buys in blocks 0 to 17 (about 10 seconds) can't sell in that time. After that, they all share one small daily limit. Splitting into many wallets doesn't give them more." },
      { b: "Big wallets sell a bit at a time.", t: "A wallet with 2% of the supply or more can sell a set amount each hour. It can't move its tokens to fresh wallets to get around that." },
      { b: "You see every big sell.", t: "When someone tries to sell over the limit, part of it goes through and the rest stays in their wallet. You get a receipt that says how much sold, and when more can follow." },
      { b: "Nobody can change the rules later.", t: "The coin has no owner, no pause button and no upgrade. What you read at launch is what runs." },
      { b: "Small bags bought after launch never hit a limit.", t: "If you buy after the early blocks and hold a normal amount, you buy and sell freely." },
    ],
  },
  creators: {
    kicker: "For creators",
    title: "Launch a coin. Get paid from trading, not from dumping.",
    items: [
      { b: "No money needed to start.", t: "You don't seed the pool. The coin launches in one transaction, and the liquidity stays locked." },
      { b: "You earn on every trade.", t: "You get a share of the swap fee. You can claim it once the pool has held a minimum of real NEAR for 24 hours, so a coin that dies on day one pays you nothing." },
      { b: "Buyers have a reason to trust you.", t: "They can read in the contract how slowly you can sell." },
    ],
    note: "The share and the minimum are not final.",
  },
  how: {
    title: "How it works",
    steps: [
      { n: "1", b: "Launch.", t: "You pick a name and a ticker. The coin goes live with the same fixed rules as every other coin." },
      { n: "2", b: "Trade.", t: "People buy and sell. Each wallet can sell a set amount per hour, and that amount grows as the pool grows." },
      { n: "3", b: "Watch.", t: "Every limited sell creates a public receipt. Our Telegram bot posts it to holders in plain words." },
    ],
  },
  wontTitle: "What it won't do",
  wontLead: "We'd rather tell you now.",
  wont: [
    "It slows a dump. It doesn't stop one.",
    "It doesn't make the price go up.",
    "Someone who buys later through many small wallets is not limited.",
  ],
  wontLink: "See every known limit",
  footer: "Testnet demo on NEAR. All the rules and the reasons behind them are in the project repository.",
};
