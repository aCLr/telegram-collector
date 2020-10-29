use log;
use simple_logger;
use std::sync::mpsc;
use tg_collector::config::Config;
use tg_collector::tg_client::{TgClient, TgUpdate};

#[tokio::main]
async fn main() {
    // simple_logger::init().unwrap();
    // log::set_max_level(log::LevelFilter::Debug);
    let conf = Config {
        log_verbosity_level: 0,
        database_directory: "tdlib".to_string(),
        api_id: env!("API_ID").parse().unwrap(),
        api_hash: env!("API_HASH").to_string(),
        phone_number: env!("TG_PHONE").to_string(),
    };
    let mut api = TgClient::new(&conf);
    let (sender, receiver) = mpsc::channel::<TgUpdate>();
    api.start_listen_updates(sender);
    api.start();
    let chats = api.search_public_chats("profunctor").await.unwrap();
    for chat in chats.chat_ids() {
        let chat = api.get_chat_history_stream(chat, ).await.unwrap();
        println!("{:?}", chat);
    }
    println!("close");

    loop {
        let update = receiver.recv().unwrap();
        println!("{:?}", update);
    }
    println!("closed");
}
