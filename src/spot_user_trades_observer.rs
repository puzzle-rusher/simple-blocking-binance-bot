use binance::api::Binance;
use binance::config::Config;
use binance::model::OrderTradeEvent;
use binance::userstream::UserStream;
use binance::websockets::{WebSockets, WebsocketEvent};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::spawn;

pub struct SpotUserTradesObserver {}

impl SpotUserTradesObserver {
    pub fn start_observing(config: &Config) -> Receiver<OrderTradeEvent> {
        let (tx, rx) = mpsc::channel();

        let config = config.clone();
        spawn(move || Self::start_websocket(config, tx));

        rx
    }

    fn start_websocket(config: Config, tx: Sender<OrderTradeEvent>) {
        let api_key = std::env::var("SPOT_API_KEY");
        let user_stream: UserStream = Binance::new_with_config(api_key.ok(), None, &config);

        if let Ok(answer) = user_stream.start() {
            let listen_key = answer.listen_key;

            let mut web_socket = WebSockets::new(|event: WebsocketEvent| {
                if let WebsocketEvent::OrderTrade(trade) = event {
                    tx.send(trade)
                        .expect("Can't send trade through the channel");
                };

                Ok(())
            });

            web_socket
                .connect_with_config(&listen_key, &config)
                .unwrap(); // check error

            if let Err(e) = web_socket.event_loop(&AtomicBool::new(true)) {
                println!("Error: {:?}", e);
            }
        } else {
            println!("Not able to start an User Stream (Check your API_KEY)");
        }
    }
}
