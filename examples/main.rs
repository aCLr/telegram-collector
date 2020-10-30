use log;
use simple_logger;
use std::sync::{mpsc, Arc, Mutex, RwLock};

use chrono::{NaiveDate, NaiveDateTime};
use tg_collector::config::Config;
use tg_collector::tg_client::{TgClient, TgUpdate};
use rtdlib::errors::RTDError;
use rtdlib::types::Message;
use futures::stream::StreamExt;
use std::borrow::BorrowMut;

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
    let main_api = Arc::new(RwLock::new(TgClient::new(&conf)));
    let (sender, _receiver) = mpsc::channel::<TgUpdate>();
    let mut join_handle = None;
    let chats = {
        println!("lock");
        let mut guard = main_api.write().unwrap();
        println!("locked");
        println!("start listen");
        guard.start_listen_updates(sender);
        println!("start");
        join_handle.replace(Some(guard.start()));
        println!("search");
        guard.search_public_chats("profunctor").await.unwrap()
    };
    println!("chats: {:?}", chats);
    let date_time: NaiveDateTime = NaiveDate::from_ymd(2017, 11, 12).and_hms(17, 33, 44);
    let mut cursor = Box::pin(TgClient::get_chat_history_stream(main_api, chats.chat_ids()[0], date_time.timestamp()));
    println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());
    // println!("{:?}", cursor.next().await.unwrap().unwrap().id());

    // println!("close");

    // loop {
    //     let update = receiver.recv().unwrap();
    //     println!("{:?}", update);
    // }
    // println!("closed");
}
