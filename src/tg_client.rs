#![allow(clippy::mutex_atomic)]
use async_trait::async_trait;
use rtdlib::types::{
    Chat, ChatType, Chats, CheckAuthenticationCode, Close, DownloadFile, File, GetChat,
    GetChatHistory, GetChats, GetMessageLink, GetSupergroup, GetSupergroupFullInfo, HttpUrl,
    JoinChat, Message, Messages, Ok, SearchPublicChats, SetAuthenticationPhoneNumber,
    SetDatabaseEncryptionKey, SetTdlibParameters, Supergroup, SupergroupFullInfo, TdlibParameters,
    UpdateAuthorizationState, UpdateChatPhoto, UpdateChatTitle, UpdateFile, UpdateMessageContent,
    UpdateNewMessage,
};
use std::io;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use telegram_client::api::aasync::AsyncApi;
use telegram_client::api::Api;
use telegram_client::client::Client;

use crate::config::Config;
use crate::result::Result;
use crate::traits;
use crate::types;
use futures::future::join_all;
use futures::{Stream, StreamExt, TryStreamExt};
use rtdlib::errors::RTDResult;
use std::collections::VecDeque;
use std::time::Duration;
use telegram_client::api::aevent::EventApi;
use telegram_client::errors::TGResult;
use telegram_client::listener::Listener;
use tokio::runtime;
use tokio::sync::{mpsc, Mutex as AsyncMutex, RwLock};

impl traits::TelegramClientTrait for Client {
    fn listener(&mut self) -> &mut Listener {
        self.listener()
    }

    fn start(&self) -> JoinHandle<()> {
        self.start()
    }
}

#[async_trait]
impl traits::TelegramAsyncApi for AsyncApi {
    async fn download_file(&self, download_file: DownloadFile) -> RTDResult<File> {
        self.download_file(download_file).await
    }

    async fn close(&self, close: Close) -> RTDResult<Ok> {
        self.close(close).await
    }

    async fn get_chat(&self, get_chat: GetChat) -> RTDResult<Chat> {
        self.get_chat(get_chat).await
    }

    async fn get_chats(&self, get_chats: GetChats) -> RTDResult<Chats> {
        self.get_chats(get_chats).await
    }

    async fn get_chat_history(&self, get_chat_history: GetChatHistory) -> RTDResult<Messages> {
        self.get_chat_history(get_chat_history).await
    }

    async fn get_message_link(&self, get_message_link: GetMessageLink) -> RTDResult<HttpUrl> {
        self.get_message_link(get_message_link).await
    }

    async fn search_public_chats(
        &self,
        search_public_chats: SearchPublicChats,
    ) -> RTDResult<Chats> {
        self.search_public_chats(search_public_chats).await
    }

    async fn join_chat(&self, join_chat: JoinChat) -> RTDResult<Ok> {
        self.join_chat(join_chat).await
    }

    async fn get_supergroup_full_info(
        &self,
        get_supergroup_full_info: GetSupergroupFullInfo,
    ) -> RTDResult<SupergroupFullInfo> {
        self.get_supergroup_full_info(get_supergroup_full_info)
            .await
    }

    async fn get_supergroup(&self, get_supergroup: GetSupergroup) -> RTDResult<Supergroup> {
        self.get_supergroup(get_supergroup).await
    }
}

#[derive(Debug, Default)]
struct DownloadQueue {
    queue_size: usize,
    queue: VecDeque<i64>,
    in_progress: Vec<i64>,
}

impl DownloadQueue {
    pub fn new(queue_size: usize) -> Self {
        Self {
            queue_size,
            ..Default::default()
        }
    }

    pub fn log_state(&self) {
        debug!("download queue state: {:?}", self);
    }

    pub fn is_in_progress(&self, obj: &i64) -> bool {
        self.in_progress.contains(&obj)
    }

    pub fn may_be_download(&mut self, obj: i64) -> bool {
        self.log_state();
        if self.in_progress.len() >= self.queue_size {
            self.queue.push_back(obj);
            false
        } else {
            self.in_progress.push(obj);
            true
        }
    }

    pub fn mark_as_done_and_get_new(&mut self, obj: &i64) -> Option<i64> {
        self.log_state();
        self.in_progress.retain(|x| x != obj);
        let new = self.queue.pop_front();
        match new {
            Some(n) => {
                self.in_progress.push(n);
                Some(n)
            }
            None => None,
        }
    }
}

#[derive(Clone)]
pub struct TgClient {
    client: Box<dyn traits::TelegramClientTrait>,
    api: Box<dyn traits::TelegramAsyncApi>,
    have_authorization: Arc<(Mutex<bool>, Condvar)>,
    download_queue: Arc<Mutex<DownloadQueue>>,
}

impl TgClient {
    pub fn new(config: &Config) -> Self {
        Client::set_log_verbosity_level(config.log_verbosity_level)
            .expect("can't change tdlib loglevel");
        let api = Box::new(Api::rasync());
        let mut client = Box::new(Client::new(api.api().clone()));
        client.warn_unregister_listener(false);
        let download_queue = Arc::new(Mutex::new(DownloadQueue::new(
            config.max_download_queue_size,
        )));

        if config.log_download_state_secs_interval != 0 {
            let q_log = download_queue.clone();
            let sleep = Duration::from_secs(config.log_download_state_secs_interval);
            tokio::spawn(async move {
                loop {
                    q_log.lock().unwrap().log_state();
                    tokio::time::delay_for(sleep).await;
                }
            });
        }
        let mut tg = TgClient {
            client,
            api,
            have_authorization: Arc::new((Mutex::new(false), Condvar::new())),
            download_queue,
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
        let channel = Arc::new(AsyncMutex::new(channel));
        listener.on_update_new_message(get_new_message_handler(channel.clone()));
        listener.on_update_message_content(get_update_content_handler(channel.clone()));
        listener.on_update_chat_photo(get_update_chat_photo_handler(channel.clone()));
        listener.on_update_chat_title(get_update_chat_title_handler(channel.clone()));

        listener.on_update_file(get_update_file_handler(
            channel,
            self.download_queue.clone(),
        ));
    }

    pub fn start(&mut self) -> JoinHandle<()> {
        let join_handle = self.client.start();
        let (lock, cvar) = &*self.have_authorization;
        let mut started = lock.lock().unwrap();
        while !*started {
            started = cvar.wait(started).unwrap();
        }
        join_handle
    }

    pub async fn get_chat(&self, chat_id: &i64) -> Result<Chat> {
        Ok(self
            .api
            .get_chat(GetChat::builder().chat_id(*chat_id).build())
            .await?)
    }

    pub async fn search_public_chats(&self, query: &str) -> Result<Vec<types::Channel>> {
        let chats = self
            .api
            .search_public_chats(SearchPublicChats::builder().query(query).build())
            .await?;
        Ok(self.convert_chats_to_channels(chats).await?)
    }

    pub async fn join_chat(&self, chat_id: &i64) -> Result<Ok> {
        Ok(self
            .api
            .join_chat(JoinChat::builder().chat_id(*chat_id).build())
            .await?)
    }

    pub async fn download_file(&mut self, file_id: i64) -> Result<()> {
        let may_be_download = {
            let mut queue = self.download_queue.lock().unwrap();
            queue.may_be_download(file_id)
        };
        if may_be_download {
            self.api
                .download_file(make_download_file_request(file_id))
                .await?;
        }
        Ok(())
    }

    pub async fn get_channel(&self, chat_id: i64) -> Result<Option<types::Channel>> {
        let chat = self
            .api
            .get_chat(GetChat::builder().chat_id(chat_id).build())
            .await?;
        match &chat.type_() {
            ChatType::Supergroup(sg) if sg.is_channel() => {
                let sg_info = self
                    .api
                    .get_supergroup_full_info(
                        GetSupergroupFullInfo::builder()
                            .supergroup_id(sg.supergroup_id())
                            .build(),
                    )
                    .await?;
                let sg = self
                    .api
                    .get_supergroup(
                        GetSupergroup::builder()
                            .supergroup_id(sg.supergroup_id())
                            .build(),
                    )
                    .await?;
                Ok(Some(types::Channel::convert(&chat, &sg, &sg_info)))
            }
            _ => Ok(None),
        }
    }

    async fn convert_chats_to_channels(&self, chats: Chats) -> Result<Vec<types::Channel>> {
        let channels = join_all(chats.chat_ids().iter().map(|&c| self.get_channel(c))).await;
        let mut result = vec![];
        for channel in channels {
            if let Some(ch) = channel? {
                result.push(ch)
            }
        }
        Ok(result)
    }

    pub async fn get_all_channels(&self, limit: i64) -> Result<Vec<types::Channel>> {
        let chats = self
            .api
            .get_chats(
                GetChats::builder()
                    .limit(limit)
                    .offset_order(9223372036854775807)
                    .build(),
            )
            .await?;
        Ok(self.convert_chats_to_channels(chats).await?)
    }

    pub async fn get_chat_history(
        &self,
        chat_id: i64,
        offset: i64,
        limit: i64,
        message_id: i64,
    ) -> Result<Messages> {
        Ok(self
            .api
            .get_chat_history(
                GetChatHistory::builder()
                    .chat_id(chat_id)
                    .offset(offset)
                    .limit(limit)
                    .from_message_id(message_id)
                    .build(),
            )
            .await?)
    }

    pub async fn get_message_link(&self, chat_id: i64, message_id: i64) -> Result<String> {
        Ok(self
            .api
            .get_message_link(
                GetMessageLink::builder()
                    .chat_id(chat_id)
                    .message_id(message_id)
                    .build(),
            )
            .await?
            .url()
            .clone())
    }

    pub fn get_chat_history_stream(
        client: Arc<RwLock<TgClient>>,
        chat_id: i64,
        date: i64,
    ) -> impl Stream<Item = Result<Message>> {
        futures::stream::unfold(
            (i64::MAX, client),
            move |(mut from_message_id, client)| async move {
                let guard = client.clone();
                let api = guard.read().await;
                let history = api.get_chat_history(chat_id, 0, 10, from_message_id).await;
                let result_messages: Result<Vec<Message>>;
                match history {
                    Ok(messages) => {
                        result_messages = Ok(messages
                            .messages()
                            .iter()
                            .filter_map(|msg| match msg {
                                None => None,
                                Some(msg) => {
                                    if msg.date() < date {
                                        None
                                    } else {
                                        if msg.id() < from_message_id {
                                            from_message_id = msg.id()
                                        }
                                        Some(msg.clone())
                                    }
                                }
                            })
                            .collect());
                    }
                    Err(err) => result_messages = Err(err),
                };
                match result_messages {
                    Ok(messages) => {
                        if !messages.is_empty() {
                            Some((Ok(messages), (from_message_id, client)))
                        } else {
                            None
                        }
                    }
                    Err(err) => Some((Err(err), (from_message_id, client))),
                }
            },
        )
        .map_ok(|updates| futures::stream::iter(updates).map(Ok))
        .try_flatten()
    }

    pub async fn close(&mut self) {
        self.api.close(Close::builder().build()).await.unwrap();
    }
}

fn make_download_file_request(file_id: i64) -> DownloadFile {
    DownloadFile::builder()
        .file_id(file_id)
        .synchronous(false)
        .priority(1)
        .build()
}

fn type_in() -> String {
    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => input.trim().to_string(),
        Err(e) => panic!("Can't get input value: {:?}", e),
    }
}

fn get_auth_state_handler(
    config: &Config,
    have_auth: Arc<(Mutex<bool>, Condvar)>,
) -> impl Fn((&EventApi, &UpdateAuthorizationState)) -> TGResult<()> + 'static {
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

macro_rules! get_handler_func {
    ($fn: ident, $orig_update_name:ident, $tg_update_name:ident) => {
        fn $fn(
            channel: Arc<AsyncMutex<mpsc::Sender<TgUpdate>>>,
        ) -> impl Fn((&EventApi, &$orig_update_name)) -> TGResult<()> + 'static {
            move |(_, update)| {
                // TODO tokio::spawn
                let mut rt = runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let mut local = channel.lock().await;
                    match local.send(TgUpdate::$tg_update_name(update.clone())).await {
                        Err(err) => warn!("{}", err),
                        Ok(_) => {}
                    };
                });
                Ok(())
            }
        }
    };
}

get_handler_func!(get_new_message_handler, UpdateNewMessage, NewMessage);
get_handler_func!(
    get_update_content_handler,
    UpdateMessageContent,
    MessageContent
);
get_handler_func!(get_update_chat_photo_handler, UpdateChatPhoto, ChatPhoto);
get_handler_func!(get_update_chat_title_handler, UpdateChatTitle, ChatTitle);

fn get_update_file_handler(
    channel: Arc<AsyncMutex<mpsc::Sender<TgUpdate>>>,
    download_queue: Arc<Mutex<DownloadQueue>>,
) -> impl Fn((&EventApi, &UpdateFile)) -> TGResult<()> + 'static {
    move |(api, update)| {
        if update.file().local().is_downloading_completed() {
            trace!("file {} downloading finished", update.file().id());
            let mut download_queue = download_queue.lock().unwrap();
            if !download_queue.is_in_progress(&update.file().id()) {
                return Ok(());
            }

            if let Some(file_id) = download_queue.mark_as_done_and_get_new(&update.file().id()) {
                trace!("file {} downloading started", file_id);
                if let Err(e) = api.download_file(make_download_file_request(file_id)) {
                    error!("{}", e);
                }
            }
            let mut rt = runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut local = channel.lock().await;
                if let Err(err) = local.send(TgUpdate::FileDownloaded(update.clone())).await {
                    error!("{}", err);
                };
            });
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum TgUpdate {
    NewMessage(UpdateNewMessage),
    MessageContent(UpdateMessageContent),
    ChatPhoto(UpdateChatPhoto),
    FileDownloaded(UpdateFile),
    ChatTitle(UpdateChatTitle),
    // looks like we do not need it: that updates may contain data, which does not make sense for project
    // there is just a several improtant fields: description, username and invite_link
    // Supergroup(UpdateSupergroup),
    // SupergroupFullInfo(UpdateSupergroupFullInfo),
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::tg_client::{get_update_file_handler, TgClient};
    use crate::traits;
    use async_trait::async_trait;
    use rtdlib::errors::{RTDError, RTDResult};
    use rtdlib::types::*;
    use std::sync::{Arc, Condvar, Mutex};
    use telegram_client::api::aevent::EventApi;
    use telegram_client::api::Api;
    use tokio::sync::{mpsc, Mutex as AsyncMutex, RwLock};

    #[derive(Clone)]
    struct MockedApi;

    #[async_trait]
    impl traits::TelegramAsyncApi for MockedApi {
        async fn download_file(&self, download_file: DownloadFile) -> RTDResult<File> {
            Ok(File::builder().build())
        }

        async fn close(&self, close: Close) -> RTDResult<Ok> {
            Ok(Ok::builder().build())
        }

        async fn get_chat(&self, get_chat: GetChat) -> RTDResult<Chat> {
            Ok(Chat::builder().build())
        }

        async fn get_chats(&self, get_chats: GetChats) -> RTDResult<Chats> {
            Ok(Chats::builder().build())
        }

        async fn get_chat_history(&self, get_chat_history: GetChatHistory) -> RTDResult<Messages> {
            Ok(Messages::builder().build())
        }

        async fn get_message_link(&self, get_message_link: GetMessageLink) -> RTDResult<HttpUrl> {
            Ok(HttpUrl::builder().build())
        }

        async fn search_public_chats(
            &self,
            search_public_chats: SearchPublicChats,
        ) -> RTDResult<Chats> {
            Ok(Chats::builder().build())
        }

        async fn join_chat(&self, join_chat: JoinChat) -> RTDResult<Ok> {
            Ok(Ok::builder().build())
        }

        async fn get_supergroup_full_info(
            &self,
            get_supergroup_full_info: GetSupergroupFullInfo,
        ) -> RTDResult<SupergroupFullInfo> {
            Ok(SupergroupFullInfo::builder().build())
        }

        async fn get_supergroup(&self, get_supergroup: GetSupergroup) -> RTDResult<Supergroup> {
            Ok(Supergroup::builder().build())
        }
    }

    #[tokio::test]
    async fn test_download_file() {
        let mut client = TgClient::new(&Config {
            max_download_queue_size: 1,
            log_verbosity_level: 0,
            database_directory: "".to_string(),
            api_id: 0,
            api_hash: "".to_string(),
            phone_number: "".to_string(),
        });
        client.api = Box::new(MockedApi);

        client.download_file(1).await;
        assert_eq!(client.download_queue.lock().unwrap().queue().len(), 0);
        assert_eq!(
            client.download_queue.lock().unwrap().in_progress(),
            &vec![1]
        );

        client.download_file(1).await;
        assert_eq!(client.download_queue.lock().unwrap().queue(), &vec![1]);
        assert_eq!(
            client.download_queue.lock().unwrap().in_progress(),
            &vec![1]
        );

        client.download_file(2).await;
        assert_eq!(client.download_queue.lock().unwrap().queue(), &vec![1, 2]);
        assert_eq!(
            client.download_queue.lock().unwrap().in_progress(),
            &vec![1]
        );

        let (sender, receiver) = mpsc::channel(1);
        let chann = Arc::new(AsyncMutex::new(sender));
        let download_finished_handler =
            get_update_file_handler(chann, client.download_queue.clone());
        let eapi = EventApi::new(Api::default());

        download_finished_handler((
            &eapi,
            &UpdateFile::builder()
                .file(
                    File::builder()
                        .local(LocalFile::builder().is_downloading_completed(false).build()),
                )
                .build(),
        ));

        // no changes
        assert_eq!(client.download_queue.lock().unwrap().queue(), &vec![1, 2]);
        assert_eq!(
            client.download_queue.lock().unwrap().in_progress(),
            &vec![1]
        );

        download_finished_handler((
            &eapi,
            &UpdateFile::builder()
                .file(
                    File::builder()
                        .id(10)
                        .local(LocalFile::builder().is_downloading_completed(true).build()),
                )
                .build(),
        ));

        // no changes: file not in progress
        assert_eq!(client.download_queue.lock().unwrap().queue(), &vec![1, 2]);
        assert_eq!(
            client.download_queue.lock().unwrap().in_progress(),
            &vec![1]
        );

        download_finished_handler((
            &eapi,
            &UpdateFile::builder()
                .file(
                    File::builder()
                        .id(1)
                        .local(LocalFile::builder().is_downloading_completed(true).build()),
                )
                .build(),
        ));
        assert_eq!(client.download_queue.lock().unwrap().queue(), &vec![2]);
        assert_eq!(
            client.download_queue.lock().unwrap().in_progress(),
            &vec![1]
        );

        download_finished_handler((
            &eapi,
            &UpdateFile::builder()
                .file(
                    File::builder()
                        .id(1)
                        .local(LocalFile::builder().is_downloading_completed(true).build()),
                )
                .build(),
        ));
        assert_eq!(client.download_queue.lock().unwrap().queue().len(), 0);
        assert_eq!(
            client.download_queue.lock().unwrap().in_progress(),
            &vec![2]
        );

        client.download_file(3).await;
        client.download_file(4).await;
        client.download_file(5).await;

        download_finished_handler((
            &eapi,
            &UpdateFile::builder()
                .file(
                    File::builder()
                        .id(2)
                        .local(LocalFile::builder().is_downloading_completed(true).build()),
                )
                .build(),
        ));

        assert_eq!(client.download_queue.lock().unwrap().queue(), &vec![4, 5]);
        assert_eq!(
            client.download_queue.lock().unwrap().in_progress(),
            &vec![3]
        );
    }
}
