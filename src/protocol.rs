use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct FileMetadata {
    pub relative_path: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerResponse {
    Send,
    Skip,
    Resume { offset: u64 },
    Error { message: String },
}
