#[derive(Debug)]
pub struct Config {
    pub max_download_queue_size: usize,
    pub log_download_state_secs_interval: u64,
    pub log_verbosity_level: i32,
    pub database_directory: String,
    pub api_id: i64,
    pub api_hash: String,
    pub phone_number: String,
}
