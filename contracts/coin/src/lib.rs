//! Fair Chance coin contract. Thin wrappers: every rule is in `fc_rules`.

use near_sdk::json_types::U128;
use near_sdk::store::LookupMap;
use near_sdk::{env, near, AccountId, NearToken, PanicOnDefault, Promise};
use rule_engine::{
    AccountView, Action, Applied, At, BuyQuote, CapView, CoinState, CreatorFeesView,
    EarlyWindowView, FixedValues, Holder, Holders, LaunchRequest, PoolView, RulesView, SellQuote,
};

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
        standard(standard = "fair_chance", version = "1.0.0-rc.5")
    )
)]
#[derive(PanicOnDefault)]
pub struct Coin {
    state: CoinState,
    holders: LookupMap<String, Holder>,
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
        description: Option<String>,
        code_hash: String,
    ) -> Self {
        let coin = env::current_account_id();
        let caller = env::predecessor_account_id();
        let launched = rule_engine::launch(
            FixedValues::fixed(),
            LaunchRequest {
                coin: coin.as_str(),
                caller: caller.as_str(),
                creator: creator.as_str(),
                code_hash: &code_hash,
                name: &name,
                symbol: &symbol,
                icon: icon.as_deref(),
                description: description.as_deref(),
            },
            now(),
        )
        .unwrap_or_else(|r| env::panic_str(&r.to_string()));
        for event in &launched.events {
            env::log_str(&event.log_line());
        }
        Self {
            state: launched.state,
            holders: LookupMap::new(b"h"),
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
        U128(self.state.fixed.total_supply)
    }

    pub fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        U128(
            self.holders
                .get(account_id.as_str())
                .map_or(0, |h| h.balance),
        )
    }

    pub fn ft_metadata(&self) -> FungibleTokenMetadata {
        let metadata = &self.state.metadata;
        FungibleTokenMetadata {
            spec: "ft-1.0.0".into(),
            name: metadata.name.clone(),
            symbol: metadata.symbol.clone(),
            icon: metadata.icon.clone(),
            reference: None,
            reference_hash: None,
            decimals: self.state.fixed.decimals,
        }
    }

    pub fn fc_rules(&self) -> RulesView {
        rule_engine::view_rules(&self.state)
    }

    pub fn fc_pool(&self) -> PoolView {
        rule_engine::view_pool(&self.state, now())
    }

    pub fn fc_early_window(&self) -> EarlyWindowView {
        rule_engine::view_early_window(&self.state, now())
    }

    pub fn fc_cap(&self) -> CapView {
        rule_engine::view_cap(&self.state, now())
    }

    pub fn fc_account(&self, account_id: AccountId) -> AccountView {
        rule_engine::view_account(&self.state, self.holders.get(account_id.as_str()), now())
    }

    pub fn fc_quote_sell(&self, account_id: AccountId, amount: U128) -> Option<SellQuote> {
        let holders = self.load(&[account_id.to_string()]);
        rule_engine::quote_sell(&self.state, holders, account_id.as_str(), amount.0, now())
    }

    pub fn fc_quote_buy(&self, account_id: AccountId, near_in: U128) -> Option<BuyQuote> {
        let holders = self.load(&[account_id.to_string()]);
        rule_engine::quote_buy(&self.state, holders, account_id.as_str(), near_in.0, now())
    }

    pub fn fc_creator_fees(&self) -> CreatorFeesView {
        rule_engine::view_creator_fees(&self.state)
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
        } = rule_engine::apply(&self.state, holders, action, now())
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
