use binance::config::Config;
use binance::model::Bids;
use binance::websockets::{WebSockets, WebsocketEvent};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread::spawn;

pub struct SpotOrderBookObserver {}

impl SpotOrderBookObserver {
    pub fn start_observing(config: &Config, symbol: &str) -> Receiver<Bids> {
        let (tx, rx) = mpsc::sync_channel(0);

        let config = config.clone();
        let symbol = symbol.to_owned();
        spawn(move || Self::start_websocket(config, tx, symbol));

        rx
    }

    fn start_websocket(config: Config, tx: SyncSender<Bids>, symbol: String) {
        let mut websocket = WebSockets::new(move |event: WebsocketEvent| {
            if let WebsocketEvent::OrderBook(account_update) = event {
                if let Some(bid) = account_update.bids.first() {
                    _ = tx.try_send(bid.clone());
                } else {
                    eprintln!("bids are empty")
                }
            };
            Ok(())
        });

        let subscription = format!("{}@depth5@100ms", symbol.to_lowercase());
        websocket
            .connect_with_config(&subscription, &config)
            .unwrap(); // check error
        websocket.event_loop(&AtomicBool::from(true)).unwrap();
    }
}
