use crate::article::Article;
use crate::ArticleScraper;

pub struct Youtube;

impl Youtube {
    pub fn handle(url: &url::Url) -> Option<Article> {
        let host_name = match ArticleScraper::get_host_name(url) {
            Ok(host_name) => host_name,
            Err(_) => return None,
        };
        if &host_name == "youtube.com" {
            let regex =
                regex::Regex::new(r#"youtube\.com/watch\?v=(.*)"#).expect("Failed to parse regex");
            if let Some(captures) = regex.captures(url.as_str()) {
                if let Some(video_id) = captures.get(1) {
                    let html = format!("<iframe width=\"650\" height=\"350\" frameborder=\"0\" src=\"https://www.youtube-nocookie.com/embed/{}\" allowfullscreen></iframe>", video_id.as_str());

                    return Some(Article {
                        title: None,
                        date: None,
                        author: None,
                        url: url.clone(),
                        html: Some(html),
                    });
                }
            }
        }

        None
    }
}
