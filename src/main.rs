mod bot_config;
mod bot_logic;
mod futures_orderbook_observer;
mod spot_orderbook_observer;
mod spot_user_trades_observer;
mod trades_executor;

use crate::bot_config::BotConfig;
use crate::bot_logic::Bot;
use crate::futures_orderbook_observer::FuturesOrderBookObserver;
use crate::spot_orderbook_observer::SpotOrderBookObserver;
use crate::spot_user_trades_observer::SpotUserTradesObserver;
use crate::trades_executor::TradesExecutor;
use binance::api::*;
use binance::config::Config;
use binance::futures::general::FuturesGeneral;
use binance::model::Filters::{MarketLotSize, MinNotional};
use dotenv::dotenv;

fn main() {
    dotenv().ok();
    let bot_config = BotConfig {
        spot_symbol: "BTCUSDT".to_string(),
        futures_symbol: "BTCUSDT".to_string(),
        min_usd_size: 140.0,
        max_usd_size: 200.0,
    };
    let config = Config::testnet();
    let spot_bids_rx = SpotOrderBookObserver::start_observing(&config, &bot_config.spot_symbol);
    let spot_trades_rx = SpotUserTradesObserver::start_observing(&config);
    let futures_bids_rx = FuturesOrderBookObserver::start_observing(&bot_config.spot_symbol);

    let general: FuturesGeneral = Binance::new_with_config(None, None, &config);
    let futures_exchange_info = general
        .exchange_info()
        .unwrap()
        .symbols
        .into_iter()
        .find(|symbol| symbol.symbol == bot_config.futures_symbol)
        .unwrap();
    let min_notional = futures_exchange_info
        .filters
        .iter()
        .filter_map(|filter| {
            if let MinNotional { notional, .. } = filter {
                notional.clone()
            } else {
                None
            }
        })
        .next()
        .map(|min_notional| min_notional.parse().unwrap());

    let min_lot_size = futures_exchange_info
        .filters
        .iter()
        .filter_map(|filter| {
            if let MarketLotSize { min_qty, .. } = filter {
                Some(min_qty)
            } else {
                None
            }
        })
        .next()
        .map(|min_notional| min_notional.parse().unwrap())
        .unwrap();

    let bot = Bot::new(
        spot_bids_rx,
        futures_bids_rx,
        spot_trades_rx,
        TradesExecutor::new(&config, &bot_config.spot_symbol, &bot_config.futures_symbol),
        bot_config.min_usd_size,
        bot_config.max_usd_size,
        10_u64.pow(futures_exchange_info.quantity_precision as u32) as f64,
        min_notional,
        min_lot_size,
    );

    bot.start().join().unwrap();
}
