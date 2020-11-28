use rtdlib::types::{Chat, Supergroup, SupergroupFullInfo};

#[derive(Debug)]
pub struct Channel {
    chat_id: i64,
    title: String,
    description: String,
    invite_link: String,
}

impl Channel {
    pub fn convert(chat: &Chat, supergroup_full_info: &SupergroupFullInfo) -> Self {
        Channel {
            chat_id: chat.id(),
            title: chat.title().clone(),
            description: supergroup_full_info.description().clone(),
            invite_link: supergroup_full_info.invite_link().clone(),
        }
    }
}
