// The words on the landing screen. The headline is the builder's. The rest follows the v5 design,
// with "first" replaced where the copy rules ban it. Rules claims match docs/OVERVIEW.md.
export type LandingAction = { label: string; opens: "demo" | "launch" | "rules" };

export const landingCopy = {
  headline: "You buy. The cabal dumps on you. You're left holding the bag.",
  subheadline: "Fair Chance slows the dump at launch, inside the coin contract.",
  intro:
    "Every coin launched here has sell limits built into its contract. The creator, the snipers and the big wallets can still sell. They just can't sell it all at once.",
  actions: [
    { label: "Watch the demo", opens: "demo" },
    { label: "Launch a coin", opens: "launch" },
    { label: "See the rules on a live coin", opens: "rules" },
  ] satisfies LandingAction[],
  // The demo video link. Until it is set, the first hero button opens the app instead.
  demoUrl: null as string | null,
  openAppLabel: "Open app",
  // A real result from the CABAL2 testnet run (evidence/testnet-cabal2-run.json): group totals read
  // from the chain, fc_early_window.shared_allowance_used.
  proof: {
    source: "From the Cabal Test 2 coin · 14 Nov, 10:12 UTC · NEAR testnet",
    sentence: "Our test cabal of 10 wallets bought within 8 seconds of launch. Together they held 3.95% of the supply. Then they tried to sell all 39.5M CABAL2 at once. 2.98M sold, 7.5% of what they asked for. The other 36.6M stayed in their wallets, because the sell limit slows early buyers.",
    coin: "cabal2.factory.fairchance.testnet",
    link: "See the coin",
  },
  // The sell receipt card on the right of the hero: the same CABAL2 group totals, from the run log
  // and the chain (evidence/testnet-cabal2-run.json and the run notes). No made-up value.
  receiptCard: {
    brand: "Fair Chance",
    tag: "Sell receipt",
    meta: "Cabal Test 2 (CABAL2) · 14 Nov, 10:12 UTC",
    who: "Our test cabal, 10 wallets",
    allowance: { label: "Shared early allowance", value: "2.98M of 9.7M used", pct: 31 },
    rows: [
      { k: "Paid in", v: "5.00 NEAR" },
      { k: "Asked", v: "39.5M CABAL2" },
      { k: "Went through", v: "2.98M CABAL2" },
      { k: "Stays in wallets", v: "36.6M CABAL2" },
      { k: "NEAR out", v: "0.32 NEAR" },
      { k: "Early sells refused", v: "10 of 10" },
    ],
    highlight: { label: "Went through", sub: "of what the cabal asked to sell, after the early window", value: "7.5%" },
    reason: "Reason: early supply shares one small allowance per 24 h. Splitting into 10 wallets gave no more.",
    foot: "NEAR testnet. Read from the chain.",
  },
  problem:
    "On most launchpads, the creator and the people who buy in the opening seconds often sell into you within minutes. A warning tool can tell you it happened. It can't stop it.",
  buyers: {
    kicker: "For buyers",
    title: "The same rules on every coin. Read them before you buy.",
    items: [
      { b: "The creator and the snipers can't sell at launch.", t: "Anyone who buys in blocks 0 to 17 (about 10 seconds at the measured mainnet block time) can't sell in that time. After that, they all share one small daily limit. Splitting into many wallets doesn't give them more." },
      { b: "Big wallets sell a bit at a time.", t: "A wallet with 2% of the supply or more can sell a set amount each hour. It can't move its tokens to fresh wallets to get around that." },
      { b: "You see every big sell.", t: "When someone tries to sell over the limit, part of it goes through and the rest stays in their wallet. You get a receipt that says how much sold, and when more can follow." },
      { b: "Nobody can change the rules later.", t: "The coin has no owner, no pause button and no upgrade. What you read at launch is what runs." },
      { b: "Small bags are not limited.", t: "One exception: when the pool cap has too little room, profit on tokens bought in the same hour is held back." },
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
    "Someone who buys later through many small wallets is not limited, except that profit on tokens bought in the same hour is held back when the pool cap has too little room.",
  ],
  wontLink: "See every known limit",
  githubLabel: "GitHub",
  githubUrl: "https://github.com/degencodebeast/fair-chance",
  footer: "Testnet demo on NEAR. All the rules and the reasons behind them are in the project repository.",
};
