use libxml::tree::Node;
use reqwest::{header::HeaderValue, Response};

pub struct ImageRequest {
    pub node: Node,
    pub http_response: Response,
    pub content_lenght: u64,
    pub content_type: Option<HeaderValue>,
}
