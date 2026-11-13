//! Fair Chance coin contract. Thin wrappers: every rule is in `fc_rules`.

use fc_rules::{
    AccountView, Action, Applied, At, BuyQuote, CapView, CoinState, CreatorFeesView,
    EarlyWindowView, Holder, Holders, PoolView, Rules, RulesView, SellQuote,
};
use near_sdk::json_types::U128;
use near_sdk::store::LookupMap;
use near_sdk::{env, near, AccountId, NearToken, PanicOnDefault, Promise};

#[derive(Clone)]
#[near(serializers = [json])]
pub struct FungibleTokenMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
    pub icon: Option<String>,
    pub reference: Option<String>,
    pub reference_hash: Option<String>,
    pub decimals: u8,
}

#[near(
    contract_state,
    contract_metadata(
        standard(standard = "nep141", version = "1.0.0"),
        standard(standard = "nep148", version = "1.0.0"),
        standard(standard = "fair_chance", version = "1.0.0-rc.4")
    )
)]
#[derive(PanicOnDefault)]
pub struct Coin {
    state: CoinState,
    holders: LookupMap<String, Holder>,
    name: String,
    symbol: String,
    icon: Option<String>,
}

fn now() -> At {
    At {
        block: env::block_height(),
        ns: env::block_timestamp(),
    }
}

#[near]
impl Coin {
    #[init]
    pub fn new(
        creator: AccountId,
        name: String,
        symbol: String,
        icon: Option<String>,
        code_hash: String,
    ) -> Self {
        let coin = env::current_account_id();
        let launched = fc_rules::launch(
            Rules::fixed(),
            coin.as_str(),
            creator.as_str(),
            &code_hash,
            now(),
        );
        for event in &launched.events {
            env::log_str(&event.log_line());
        }
        Self {
            state: launched.state,
            holders: LookupMap::new(b"h"),
            name,
            symbol,
            icon,
        }
    }

    /// Buy tokens with the attached NEAR. Refused with `FC_SLIPPAGE` below `min_tokens_out`.
    #[payable]
    pub fn buy(&mut self, min_tokens_out: U128) {
        let action = Action::Buy {
            account: env::predecessor_account_id().to_string(),
            near_in: env::attached_deposit().as_yoctonear(),
            min_tokens_out: min_tokens_out.0,
        };
        self.commit(&action);
    }

    /// Sell `amount` tokens. Needs exactly 1 yoctoNEAR. `min_near_out` is the NEAR accepted for
    /// all of `amount`; the check is on the price per token of the part that fills.
    #[payable]
    pub fn sell(&mut self, amount: U128, min_near_out: U128) -> Option<Promise> {
        let action = Action::Sell {
            account: env::predecessor_account_id().to_string(),
            amount: amount.0,
            min_near_out: min_near_out.0,
            attached: env::attached_deposit().as_yoctonear(),
        };
        self.commit(&action)
    }

    pub fn ft_total_supply(&self) -> U128 {
        U128(self.state.rules.total_supply)
    }

    pub fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        U128(
            self.holders
                .get(account_id.as_str())
                .map_or(0, |h| h.balance),
        )
    }

    pub fn ft_metadata(&self) -> FungibleTokenMetadata {
        FungibleTokenMetadata {
            spec: "ft-1.0.0".into(),
            name: self.name.clone(),
            symbol: self.symbol.clone(),
            icon: self.icon.clone(),
            reference: None,
            reference_hash: None,
            decimals: self.state.rules.decimals,
        }
    }

    pub fn fc_rules(&self) -> RulesView {
        fc_rules::view_rules(&self.state)
    }

    pub fn fc_pool(&self) -> PoolView {
        fc_rules::view_pool(&self.state, now())
    }

    pub fn fc_early_window(&self) -> EarlyWindowView {
        fc_rules::view_early_window(&self.state, now())
    }

    pub fn fc_cap(&self) -> CapView {
        fc_rules::view_cap(&self.state, now())
    }

    pub fn fc_account(&self, account_id: AccountId) -> AccountView {
        fc_rules::view_account(&self.state, self.holders.get(account_id.as_str()), now())
    }

    pub fn fc_quote_sell(&self, account_id: AccountId, amount: U128) -> Option<SellQuote> {
        let holders = self.load(&[account_id.to_string()]);
        fc_rules::quote_sell(&self.state, holders, account_id.as_str(), amount.0, now())
    }

    pub fn fc_quote_buy(&self, account_id: AccountId, near_in: U128) -> Option<BuyQuote> {
        let holders = self.load(&[account_id.to_string()]);
        fc_rules::quote_buy(&self.state, holders, account_id.as_str(), near_in.0, now())
    }

    pub fn fc_creator_fees(&self) -> CreatorFeesView {
        fc_rules::view_creator_fees(&self.state)
    }
}

impl Coin {
    fn load(&self, accounts: &[String]) -> Holders {
        accounts
            .iter()
            .filter_map(|a| self.holders.get(a).map(|h| (a.clone(), h.clone())))
            .collect()
    }

    /// Run one rule step: panic on refusal, else store, log and pay.
    fn commit(&mut self, action: &Action) -> Option<Promise> {
        let holders = self.load(&action.accounts());
        let Applied {
            state,
            holders,
            events,
            payout,
            ..
        } = fc_rules::apply(&self.state, holders, action, now())
            .unwrap_or_else(|r| env::panic_str(&r.to_string()));
        self.state = state;
        for (account, holder) in holders {
            self.holders.insert(account, holder);
        }
        for event in &events {
            env::log_str(&event.log_line());
        }
        payout.map(|p| {
            let to: AccountId = p
                .account
                .parse()
                .unwrap_or_else(|_| env::panic_str("bad payout account"));
            Promise::new(to).transfer(NearToken::from_yoctonear(p.amount))
        })
    }
}
