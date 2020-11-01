use log;
use simple_logger;
use std::sync::Arc;

use chrono::{NaiveDate, NaiveDateTime};
use futures::stream::StreamExt;
use tg_collector::config::Config;
use tg_collector::tg_client::{TgClient, TgUpdate};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::spawn;

#[tokio::main]
async fn main() {
    simple_logger::init().unwrap();
    log::set_max_level("DEBUG".parse().unwrap());
    let conf = Config {
        log_verbosity_level: 0,
        database_directory: "tdlib".to_string(),
        api_id: env!("API_ID").parse().unwrap(),
        api_hash: env!("API_HASH").to_string(),
        phone_number: env!("TG_PHONE").to_string(),
    };
    let main_api = Arc::new(RwLock::new(TgClient::new(&conf)));
    let (sender, receiver) = mpsc::channel::<TgUpdate>(2000);
    let mut join_handle = None;
    let chats = {
        println!("lock");
        let mut guard = main_api.write().await;
        println!("locked");
        println!("start listen");
        guard.start_listen_updates(sender);
        println!("start");
        join_handle.replace(Some(guard.start()));
        println!("search");
        guard.search_public_chats("profunctor").await.unwrap()
    };
    println!("chats: {:?}", chats);
    let date_time: NaiveDateTime = NaiveDate::from_ymd(2020, 10, 26).and_hms(9, 15, 52);
    let mut cursor = Box::pin(TgClient::get_chat_history_stream(
        main_api,
        chats.chat_ids()[0],
        date_time.timestamp(),
    ));
    let receiver = Mutex::new(receiver);
    spawn(async move {
        loop {
            let update = receiver.lock().await.recv().await;
            println!("{:?}", update);
        }
    });
    while let Some(message) = cursor.next().await {
        println!(
            "{:?}",
            NaiveDateTime::from_timestamp(message.unwrap().date(), 0)
        );
    }
}
