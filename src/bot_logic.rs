use crate::trades_executor::TradesExecutor;
use binance::model::Bids;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::thread::JoinHandle;

pub struct Bot {
    current_spot_bid_price: Mutex<f64>,
    current_futures_bid_size: Mutex<f64>,
    spot_user_trades_rx: Mutex<Receiver<binance::model::OrderTradeEvent>>,
    size_condvar: Condvar,
    trades_executor: TradesExecutor,
    futures_order_min_base_size: f64,
    min_quote_size: f64,
    max_quote_size: f64,
    futures_size_precision_coefficient: f64,
}

impl Bot {
    pub fn new(
        spot_bid_rx: Receiver<Bids>,
        futures_bid_rx: Receiver<Bids>,
        spot_user_trades_rx: Receiver<binance::model::OrderTradeEvent>,
        trades_executor: TradesExecutor,
        min_quote_size: f64,
        max_quote_size: f64,
        futures_size_precision_coefficient: f64,
        futures_symbol_min_notional: Option<f64>,
        futures_min_lot_size: f64,
    ) -> Arc<Self> {
        let min_quote_size = min_quote_size.max(futures_symbol_min_notional.unwrap_or(0.0));

        if max_quote_size <= min_quote_size {
            panic!("max_quote_size is too low")
        }

        let this = {
            let spot_bid = spot_bid_rx.recv().unwrap();
            let futures_bid = futures_bid_rx.recv().unwrap();
            let futures_order_min_base_size = futures_symbol_min_notional
                .map(|min_notional| min_notional / futures_bid.price)
                .map(|min_base| Self::ceil_number(min_base, futures_size_precision_coefficient))
                .unwrap_or(futures_min_lot_size)
                .max(futures_min_lot_size);

            Self {
                current_spot_bid_price: Mutex::new(spot_bid.price),
                current_futures_bid_size: Mutex::new(futures_bid.qty),
                spot_user_trades_rx: Mutex::new(spot_user_trades_rx),
                size_condvar: Condvar::new(),
                trades_executor,
                futures_order_min_base_size,
                min_quote_size,
                max_quote_size,
                futures_size_precision_coefficient,
            }
        };

        let this = Arc::new(this);

        {
            let this = this.clone();
            thread::spawn(move || {
                let new_value = spot_bid_rx.recv().unwrap().price;
                *(this.current_spot_bid_price.lock().unwrap()) = new_value;
            });
        }

        {
            let this = this.clone();
            thread::spawn(move || {
                let new_value = futures_bid_rx.recv().unwrap().qty;
                *this.current_futures_bid_size.lock().unwrap() = new_value;
                this.size_condvar.notify_one();
            });
        }

        this
    }

    pub fn start(self: Arc<Self>) -> JoinHandle<()> {
        thread::spawn(move || {
            loop {
                self.execute_the_flow();
            }
        })
    }

    fn execute_the_flow(&self) {
        let (limit_order_price, limit_order_size) = self.deduce_spot_order_params();

        let spot_order_id = loop {
            match self
                .trades_executor
                .spot_limit_buy(limit_order_size, limit_order_price)
            {
                Ok(transaction) => {
                    break transaction.order_id;
                }
                Err(err) => {
                    eprintln!("{}", err);
                }
            }
        };

        println!(
            "Order_{}: LIMIT BUY {} BTCUSDT",
            spot_order_id, limit_order_size
        );

        let mut available_to_sell = 0.0;

        loop {
            let trade = self.spot_user_trades_rx.lock().unwrap().recv().unwrap();
            if trade.order_id == spot_order_id {
                match trade.order_status.as_str() {
                    "PARTIALLY_FILLED" => {
                        println!(
                            "Order_{}: PARTIALLY FILLED {} BTC",
                            spot_order_id, trade.qty_last_filled_trade
                        );
                        available_to_sell += trade.qty_last_filled_trade.parse::<f64>().unwrap();
                        available_to_sell -= self.create_futures_market_order(
                            available_to_sell,
                            trade.qty.parse::<f64>().unwrap()
                                - trade.accumulated_qty_filled_trades.parse::<f64>().unwrap(),
                        );
                    }
                    "FILLED" => {
                        println!(
                            "Order_{}: FILLED {} BTC",
                            spot_order_id, trade.qty_last_filled_trade
                        );
                        available_to_sell += trade.qty_last_filled_trade.parse::<f64>().unwrap();
                        self.create_futures_market_order(available_to_sell, 0.0);
                        break;
                    }
                    "NEW" => {}
                    state => println!("Unprocessed state: {state}"),
                }
            }
        }
    }

    /// returns limit_order_price and limit_order_size
    fn deduce_spot_order_params(&self) -> (f64, f64) {
        let mut limit_order_price = *self.current_spot_bid_price.lock().unwrap();
        let mut limit_order_size = self.current_futures_bid_size.lock().unwrap();

        while limit_order_price * *limit_order_size < self.min_quote_size {
            limit_order_size = self.size_condvar.wait(limit_order_size).unwrap();
            *limit_order_size =
                Self::floor_number(*limit_order_size, self.futures_size_precision_coefficient);
            limit_order_price = *self.current_spot_bid_price.lock().unwrap();
        }

        let limit_order_size = (*limit_order_size).min(self.max_quote_size / limit_order_price);
        let limit_order_size =
            Self::floor_number(limit_order_size, self.futures_size_precision_coefficient);

        (limit_order_price, limit_order_size)
    }

    /// returns market order size, if order is not published it'll return zero
    fn create_futures_market_order(
        &self,
        available_to_sell: f64,
        remains_to_buy_on_spot: f64,
    ) -> f64 {
        if let Some(futures_order_size) =
            self.calculate_size_to_sell_on_futures(available_to_sell, remains_to_buy_on_spot)
        {
            match self.trades_executor.futures_market_sell(futures_order_size) {
                Ok(futures_order) => {
                    println!(
                        "Order_{}: MARKET SELL {} BTCUSDT_PERP",
                        futures_order.order_id, futures_order.orig_qty
                    );
                    return futures_order.orig_qty;
                }
                Err(err) => {
                    eprintln!("Couldn't create futures market order, err: {err}")
                }
            }
        }

        0.0
    }

    fn calculate_size_to_sell_on_futures(
        &self,
        mut available_to_sell: f64,
        remains_to_buy: f64,
    ) -> Option<f64> {
        if remains_to_buy == 0.0 {
            return Some(available_to_sell);
        }

        if remains_to_buy < self.futures_order_min_base_size {
            available_to_sell -= self.futures_order_min_base_size - remains_to_buy;
        }

        if available_to_sell < self.futures_order_min_base_size {
            return None;
        }

        Some(Self::floor_number(
            available_to_sell,
            self.futures_size_precision_coefficient,
        ))
    }

    fn floor_number(number: f64, precision: f64) -> f64 {
        let number = (number * precision) as u64;
        number as f64 / precision
    }

    fn ceil_number(number: f64, precision: f64) -> f64 {
        let number = (number * precision).ceil() as u64;
        number as f64 / precision
    }
}
