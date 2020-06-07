use crate::article::Article;

pub struct Youtube;

impl Youtube {
    pub fn handle(url: &url::Url) -> Option<Article> {
        if url.host_str() == Some("youtube.com") || url.host_str() == Some("www.youtube.com") {
            let regex = regex::Regex::new(r#"youtube\.com/watch\?v=(.*)"#).unwrap();
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
