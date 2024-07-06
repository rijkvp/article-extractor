#[derive(Debug, Clone)]
pub struct ImageData {
    pub url: String,
    pub data: Vec<u8>,
    pub content_type: String,
}

#[derive(Debug, Clone)]
pub struct ImageDataBase64 {
    pub url: String,
    pub data: String,
}
