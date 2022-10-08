use chrono::{DateTime, Utc};
use std::fs::File;
use std::io::{Error, ErrorKind, Write};
use std::path::PathBuf;
use url::Url;

pub struct Article {
    pub title: Option<String>,
    pub author: Option<String>,
    pub url: Url,
    pub date: Option<DateTime<Utc>>,
    pub html: Option<String>,
}

impl Article {
    pub fn save_html(&self, path: &PathBuf) -> Result<(), Error> {
        if let Some(ref html) = self.html {
            if let Ok(()) = std::fs::create_dir_all(&path) {
                let mut file_name = match self.title.clone() {
                    Some(file_name) => file_name.replace('/', "_"),
                    None => "Unknown Title".to_owned(),
                };
                file_name.push_str(".html");
                let path = path.join(file_name);
                let mut html_file = File::create(&path)?;
                html_file.write_all(html.as_bytes())?;
                return Ok(());
            }
        }

        Err(Error::new(
            ErrorKind::NotFound,
            "Article does not contain HTML",
        ))
    }
}
