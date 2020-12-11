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
    log::set_max_level("INFO".parse().unwrap());
    let conf = Config {
        max_download_queue_size: 1,
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
        let s = guard.search_public_chats("profunctor").await;
        println!("{:?}", s);
        s.unwrap()
    };
    println!("chats: {:?}", chats);

    let all_chats = main_api.read().await.get_all_channels(10).await;
    println!("{:?}", all_chats);
    //
    // let receiver = Mutex::new(receiver);
    // spawn(async move {
    //     loop {
    //         let update = receiver.lock().await.recv().await;
    //         println!("{:?}", update);
    //     }
    // });
    // while let Some(message) = cursor.next().await {
    //     println!(
    //         "{:?}",
    //         NaiveDateTime::from_timestamp(message.unwrap().date(), 0)
    //     );
    // }
}
