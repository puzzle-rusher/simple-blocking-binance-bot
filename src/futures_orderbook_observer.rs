use binance::futures::websockets::{FuturesMarket, FuturesWebSockets, FuturesWebsocketEvent};
use binance::model::Bids;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread::spawn;

pub struct FuturesOrderBookObserver {}

impl FuturesOrderBookObserver {
    pub fn start_observing(symbol: &str) -> Receiver<Bids> {
        let (tx, rx) = mpsc::sync_channel(0);

        let symbol = symbol.to_owned();
        spawn(move || Self::start_websocket(tx, symbol));

        rx
    }

    fn start_websocket(tx: SyncSender<Bids>, symbol: String) {
        let mut websocket = FuturesWebSockets::new(move |event: FuturesWebsocketEvent| {
            if let FuturesWebsocketEvent::DepthOrderBook(account_update) = event {
                if let Some(bid) = account_update.bids.first() {
                    _ = tx.try_send(bid.clone());
                } else {
                    eprintln!("bids are empty")
                }
            }

            Ok(())
        });

        let subscription = format!("{}@depth5@100ms", symbol.to_lowercase());
        websocket
            .connect(&FuturesMarket::USDM, subscription.leak())
            .unwrap(); // check error
        websocket.event_loop(&AtomicBool::from(true)).unwrap();
    }
}
