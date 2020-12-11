use async_trait::async_trait;
use dyn_clone::DynClone;
use rtdlib::errors::RTDResult;
use rtdlib::types::{
    Chat, Chats, Close, DownloadFile, File, GetChat, GetChatHistory, GetChats, GetMessageLink,
    GetSupergroup, GetSupergroupFullInfo, HttpUrl, JoinChat, Messages, Ok, SearchPublicChats,
    Supergroup, SupergroupFullInfo,
};
use std::thread::JoinHandle;
use telegram_client::listener::Listener;

pub trait TelegramClientTrait: DynClone + Send + Sync {
    fn listener(&mut self) -> &mut Listener;
    fn start(&self) -> JoinHandle<()>;
}

dyn_clone::clone_trait_object!(TelegramClientTrait);

#[async_trait]
pub trait TelegramAsyncApi: DynClone + Send + Sync {
    async fn download_file(&self, download_file: DownloadFile) -> RTDResult<File>;
    async fn close(&self, close: Close) -> RTDResult<Ok>;
    async fn get_chat(&self, get_chat: GetChat) -> RTDResult<Chat>;
    async fn get_chats(&self, get_chats: GetChats) -> RTDResult<Chats>;
    async fn get_chat_history(&self, get_chat_history: GetChatHistory) -> RTDResult<Messages>;
    async fn get_message_link(&self, get_message_link: GetMessageLink) -> RTDResult<HttpUrl>;
    async fn search_public_chats(&self, search_public_chats: SearchPublicChats)
        -> RTDResult<Chats>;
    async fn join_chat(&self, join_chat: JoinChat) -> RTDResult<Ok>;
    async fn get_supergroup_full_info(
        &self,
        get_supergroup_full_info: GetSupergroupFullInfo,
    ) -> RTDResult<SupergroupFullInfo>;
    async fn get_supergroup(&self, get_supergroup: GetSupergroup) -> RTDResult<Supergroup>;
}
dyn_clone::clone_trait_object!(TelegramAsyncApi);
