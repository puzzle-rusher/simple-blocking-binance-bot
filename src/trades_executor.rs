use binance::account::Account;
use binance::api::Binance;
use binance::config::Config;
use binance::futures::account::FuturesAccount;
use binance::{futures, model};
use std::env;

pub struct TradesExecutor {
    futures_symbol: String,
    spot_symbol: String,
    futures_account: FuturesAccount,
    spot_account: Account,
}

impl TradesExecutor {
    pub fn new(config: &Config, spot_symbol: &str, futures_symbol: &str) -> Self {
        let futures_account: FuturesAccount = Binance::new_with_config(
            Some(env::var("FUTURES_API_KEY").unwrap()),
            Some(env::var("FUTURES_SECRET_KEY").unwrap()),
            config,
        );
        let spot_account: Account = Binance::new_with_config(
            Some(env::var("SPOT_API_KEY").unwrap()),
            Some(env::var("SPOT_SECRET_KEY").unwrap()),
            config,
        );

        Self {
            futures_symbol: futures_symbol.to_owned(),
            spot_symbol: spot_symbol.to_owned(),
            futures_account,
            spot_account,
        }
    }

    pub fn futures_market_sell(
        &self,
        size: f64,
    ) -> binance::errors::Result<futures::model::Transaction> {
        self.futures_account.market_sell(&self.futures_symbol, size)
    }

    pub fn spot_limit_buy(
        &self,
        size: f64,
        price: f64,
    ) -> binance::errors::Result<model::Transaction> {
        self.spot_account.limit_buy(&self.spot_symbol, size, price)
    }
}
