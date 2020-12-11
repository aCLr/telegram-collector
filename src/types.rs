use rtdlib::types::{Chat, Supergroup, SupergroupFullInfo};

#[derive(Debug)]
pub struct Channel {
    pub chat_id: i64,
    pub title: String,
    pub description: String,
    pub username: String,
}

impl Channel {
    pub fn convert(
        chat: &Chat,
        supergroup: &Supergroup,
        supergroup_full_info: &SupergroupFullInfo,
    ) -> Self {
        Channel {
            chat_id: chat.id(),
            title: chat.title().clone(),
            description: supergroup_full_info.description().clone(),
            username: supergroup.username().to_string(),
        }
    }
}
