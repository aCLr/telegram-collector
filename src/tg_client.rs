use rtdlib::errors::{RTDError, RTDResult};
use rtdlib::types::{
    Chat, Chats, CheckAuthenticationCode, Close, GetChat, GetChatHistory, JoinChat, Message,
    Messages, Ok, SearchPublicChats, SetAuthenticationPhoneNumber, SetDatabaseEncryptionKey,
    SetTdlibParameters, TdlibParameters, Update, UpdateAuthorizationState, UpdateChatPhoto,
    UpdateChatTitle, UpdateMessageContent, UpdateNewMessage, UpdateSupergroup,
    UpdateSupergroupFullInfo,
};
use std::io;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use telegram_client::api::aasync::AsyncApi;
use telegram_client::api::Api;
use telegram_client::client::Client;
use telegram_client::listener::Listener;

use crate::config::Config;
use futures::task::{Context, Poll};
use futures::{Future, Stream, StreamExt, TryStreamExt};
use std::borrow::Borrow;
use telegram_client::api::aevent::EventApi;
use telegram_client::errors::{TGError, TGResult};
use std::pin::Pin;

pub struct TgClient {
    client: Client,
    api: AsyncApi,
    join_handle: Option<JoinHandle<()>>,
    have_authorization: Arc<(Mutex<bool>, Condvar)>,
}

impl TgClient {
    pub fn new(config: &Config) -> Self {
        Client::set_log_verbosity_level(config.log_verbosity_level);
        let api = Api::rasync();
        let mut client = Client::new(api.api().clone());
        client.warn_unregister_listener(false);
        let mut tg = TgClient {
            client,
            api: api,
            join_handle: None,
            have_authorization: Arc::new((Mutex::new(false), Condvar::new())),
        };
        tg.auth(config);
        tg
    }

    fn auth(&mut self, config: &Config) {
        let listener = self.client.listener();

        listener.on_update_authorization_state(get_auth_state_handler(
            config,
            self.have_authorization.clone(),
        ));
    }

    pub fn start_listen_updates(&mut self, channel: mpsc::Sender<TgUpdate>) {
        let listener = self.client.listener();
        let channel = Arc::new(Mutex::new(channel));
        listener.on_update_new_message(get_new_message_handler(channel.clone()));
        listener.on_update_message_content(get_update_content_handler(channel.clone()));
        listener.on_update_chat_photo(get_update_chat_photo_handler(channel.clone()));
        listener.on_update_chat_title(get_update_chat_title_handler(channel.clone()));
        listener.on_update_supergroup(get_update_supergroup_handler(channel.clone()));
        listener.on_update_supergroup_full_info(get_update_supergroup_full_info_handler(
            channel.clone(),
        ));
    }

    pub fn start(&mut self) {
        self.join_handle = Some(self.client.start());

        let (lock, cvar) = &*self.have_authorization;
        let mut started = lock.lock().unwrap();
        while !*started {
            started = cvar.wait(started).unwrap();
        }
    }

    pub async fn get_chat(&self, chat_id: &i64) -> RTDResult<Chat> {
        self.api
            .get_chat(GetChat::builder().chat_id(*chat_id).build())
            .await
    }

    pub async fn search_public_chats(&self, query: &str) -> RTDResult<Chats> {
        self.api
            .search_public_chats(SearchPublicChats::builder().query(query).build())
            .await
    }

    pub async fn join_chat(&self, chat_id: &i64) -> RTDResult<(Ok)> {
        self.api
            .join_chat(JoinChat::builder().chat_id(*chat_id).build())
            .await
    }

    pub async fn get_chat_history(&self, chat_id: i64, offset: i64, message_id: i64) -> RTDResult<Messages> {
        self.api
            .get_chat_history(GetChatHistory::builder().chat_id(chat_id).offset(offset).from_message_id(message_id).build())
            .await
    }

    pub async fn get_chat_history_stream(self, chat_id: i64, date: i64)-> impl Stream<Item = Result<Option<Message>, RTDError>> {
        let api = Arc::new(self).clone();
        futures::stream::unfold(
            (0, api),
            move |(from_message_id, api)| async move {
                let history = api.get_chat_history(chat_id, -99, from_message_id).await;
                let from_message_id = match &history {
                    Ok(messages) => {Ok(messages.messages().iter().map(|m|m.as_ref().unwrap().id()).min().map(|min_message_id|min_message_id - 1))}
                    Err(err) => {Err(err)}
                };
                match from_message_id {
                    Ok(from_message_id) => {Some((history, (from_message_id.unwrap(), api)))}
                    Err(err) => {None}
                }
            },
        )
        .map_ok(|updates| futures::stream::iter(updates.messages().clone()).map(Ok))
        .try_flatten()
    }

    pub async fn close(&mut self) {
        self.api.close(Close::builder().build()).await.unwrap();
        if let Some(join_handle) = self.join_handle.take() {
            join_handle.join().unwrap();
        }
    }
}

fn type_in() -> String {
    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => input.trim().to_string(),
        Err(e) => panic!("Can not get input value: {:?}", e),
    }
}

fn get_auth_state_handler(
    config: &Config,
    have_auth: Arc<(Mutex<bool>, Condvar)>,
) -> impl Fn((&EventApi, &UpdateAuthorizationState)) -> TGResult<()> + 'static {
    let have_auth = have_auth.clone();
    let td_params = SetTdlibParameters::builder()
        .parameters(
            TdlibParameters::builder()
                .database_directory(&config.database_directory)
                .use_test_dc(false)
                .api_id(config.api_id)
                .api_hash(&config.api_hash)
                .system_language_code("en")
                .device_model("Desktop")
                .system_version("Unknown")
                .application_version(env!("CARGO_PKG_VERSION"))
                .enable_storage_optimizer(true)
                .build(),
        )
        .build();
    let phone_number = SetAuthenticationPhoneNumber::builder()
        .phone_number(&config.phone_number)
        .build();
    move |(api, update)| {
        let state = update.authorization_state();
        state.on_wait_tdlib_parameters(|_| {
            api.set_tdlib_parameters(&td_params).unwrap();
            debug!("Set tdlib parameters");
        });
        state.on_wait_encryption_key(|_| {
            let params = SetDatabaseEncryptionKey::builder().build();
            api.set_database_encryption_key(&params).unwrap();
            debug!("Set encryption key");
        });
        state.on_wait_phone_number(|_| {
            api.set_authentication_phone_number(&phone_number).unwrap();
            debug!("Set phone number");
        });
        state.on_wait_code(|_| {
            println!("wait for auth code");
            let code = type_in();
            let code = CheckAuthenticationCode::builder().code(&code).build();
            api.check_authentication_code(&code).unwrap();
            debug!("Set auth code");
        });
        state.on_ready(|_| {
            let (lock, cvar) = &*have_auth;
            let mut authorized = lock.lock().unwrap();
            *authorized = true;
            cvar.notify_one();
        });
        Ok(())
    }
}

fn get_new_message_handler(
    channel: Arc<Mutex<mpsc::Sender<TgUpdate>>>,
) -> impl Fn((&EventApi, &UpdateNewMessage)) -> TGResult<()> + 'static {
    move |(api, update)| {
        let local = channel.lock().unwrap();
        local.send(TgUpdate::NewMessage(update.clone())).unwrap();
        Ok(())
    }
}

fn get_update_content_handler(
    channel: Arc<Mutex<mpsc::Sender<TgUpdate>>>,
) -> impl Fn((&EventApi, &UpdateMessageContent)) -> TGResult<()> + 'static {
    move |(api, update)| {
        let local = channel.lock().unwrap();
        local
            .send(TgUpdate::MessageContent(update.clone()))
            .unwrap();
        Ok(())
    }
}

fn get_update_chat_photo_handler(
    channel: Arc<Mutex<mpsc::Sender<TgUpdate>>>,
) -> impl Fn((&EventApi, &UpdateChatPhoto)) -> TGResult<()> + 'static {
    move |(api, update)| {
        let local = channel.lock().unwrap();
        local.send(TgUpdate::ChatPhoto(update.clone())).unwrap();
        Ok(())
    }
}

fn get_update_chat_title_handler(
    channel: Arc<Mutex<mpsc::Sender<TgUpdate>>>,
) -> impl Fn((&EventApi, &UpdateChatTitle)) -> TGResult<()> + 'static {
    move |(api, update)| {
        let local = channel.lock().unwrap();
        local.send(TgUpdate::ChatTitle(update.clone())).unwrap();
        Ok(())
    }
}

fn get_update_supergroup_handler(
    channel: Arc<Mutex<mpsc::Sender<TgUpdate>>>,
) -> impl Fn((&EventApi, &UpdateSupergroup)) -> TGResult<()> + 'static {
    move |(api, update)| {
        let local = channel.lock().unwrap();
        local.send(TgUpdate::Supergroup(update.clone())).unwrap();
        Ok(())
    }
}
fn get_update_supergroup_full_info_handler(
    channel: Arc<Mutex<mpsc::Sender<TgUpdate>>>,
) -> impl Fn((&EventApi, &UpdateSupergroupFullInfo)) -> TGResult<()> + 'static {
    move |(api, update)| {
        let local = channel.lock().unwrap();
        local
            .send(TgUpdate::SupergroupFullInfo(update.clone()))
            .unwrap();
        Ok(())
    }
}

#[derive(Debug)]
pub enum TgUpdate {
    NewMessage(UpdateNewMessage),
    MessageContent(UpdateMessageContent),
    ChatPhoto(UpdateChatPhoto),
    ChatTitle(UpdateChatTitle),
    Supergroup(UpdateSupergroup),
    SupergroupFullInfo(UpdateSupergroupFullInfo),
}
