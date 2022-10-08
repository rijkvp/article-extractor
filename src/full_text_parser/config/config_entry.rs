use crate::util::Util;

use super::error::{ConfigError, ConfigErrorKind};
use failure::ResultExt;
use std::borrow::Cow;
use std::io::Cursor;
use std::path::Path;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

#[derive(Clone)]
pub struct Replace {
    pub to_replace: String,
    pub replace_with: String,
}

#[derive(Clone)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Clone)]
pub struct ConfigEntry {
    pub xpath_title: Vec<String>,
    pub xpath_author: Vec<String>,
    pub xpath_date: Vec<String>,
    pub xpath_body: Vec<String>,
    pub xpath_strip: Vec<String>,
    pub strip_id_or_class: Vec<String>,
    pub strip_image_src: Vec<String>,
    pub replace: Vec<Replace>,
    pub header: Vec<Header>,
    pub single_page_link: Option<String>,
    pub next_page_link: Option<String>,
}

impl ConfigEntry {
    pub async fn parse_path(config_path: &Path) -> Result<ConfigEntry, ConfigError> {
        let mut file = fs::File::open(&config_path)
            .await
            .context(ConfigErrorKind::IO)?;
        let buffer = BufReader::new(&mut file);

        Self::parse(buffer).await
    }

    pub async fn parse_data(data: Cow<'static, [u8]>) -> Result<ConfigEntry, ConfigError> {
        let data = data.as_ref();
        let mut cursor = Cursor::new(data);
        let buffer = BufReader::new(&mut cursor);

        Self::parse(buffer).await
    }

    async fn parse<R: AsyncRead + Unpin>(buffer: BufReader<R>) -> Result<ConfigEntry, ConfigError> {
        let mut xpath_title: Vec<String> = Vec::new();
        let mut xpath_author: Vec<String> = Vec::new();
        let mut xpath_date: Vec<String> = Vec::new();
        let mut xpath_body: Vec<String> = Vec::new();
        let mut xpath_strip: Vec<String> = Vec::new();
        let mut strip_id_or_class: Vec<String> = Vec::new();
        let mut strip_image_src: Vec<String> = Vec::new();
        let mut replace_vec: Vec<Replace> = Vec::new();
        let mut header_vec: Vec<Header> = Vec::new();
        let mut next_page_link: Option<String> = None;
        let mut single_page_link: Option<String> = None;

        // ignore: tidy, prune, autodetect_on_failure and test_url
        let title = "title:";
        let body = "body:";
        let date = "date:";
        let author = "author:";
        let strip = "strip:";
        let strip_id = "strip_id_or_class:";
        let strip_img = "strip_image_src:";
        let single_page = "single_page_link:";
        let next_page = "next_page_link:";
        let find = "find_string:";
        let replace = "replace_string:";
        let replace_single = "replace_string(";
        let http_header = "http_header(";

        // ignore these
        let tidy = "tidy:";
        let prune = "prune:";
        let test_url = "test_url:";
        let autodetect = "autodetect_on_failure:";

        let mut lines = buffer.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim();
            if line.starts_with('#')
                || line.starts_with(tidy)
                || line.starts_with(prune)
                || line.starts_with(test_url)
                || line.starts_with(autodetect)
                || line.is_empty()
            {
                continue;
            }

            extract_vec_multi!(line, title, xpath_title);
            extract_vec_multi!(line, body, xpath_body);
            extract_vec_multi!(line, date, xpath_date);
            extract_vec_multi!(line, author, xpath_author);

            extract_vec_single!(line, strip, xpath_strip);
            extract_vec_single!(line, strip_id, strip_id_or_class);
            extract_vec_single!(line, strip_img, strip_image_src);

            extract_option_single!(line, single_page, single_page_link);
            extract_option_single!(line, next_page, next_page_link);

            if line.starts_with(replace_single) {
                let value = Util::str_extract_value(replace_single, line);
                let value: Vec<&str> = value.split("): ").map(|s| s.trim()).collect();
                if value.len() != 2 {
                    continue;
                }

                if let Some(to_replace) = value.first() {
                    if let Some(replace_with) = value.get(1) {
                        replace_vec.push(Replace {
                            to_replace: (*to_replace).to_string(),
                            replace_with: (*replace_with).to_string(),
                        });
                    }
                }

                continue;
            }

            if line.starts_with(http_header) {
                let value = Util::str_extract_value(http_header, line);
                let value: Vec<&str> = value.split("): ").map(|s| s.trim()).collect();
                if value.len() != 2 {
                    continue;
                }

                if let Some(name) = value.first() {
                    if let Some(value) = value.get(1) {
                        header_vec.push(Header {
                            name: (*name).to_string(),
                            value: (*value).to_string(),
                        });
                    }
                }

                continue;
            }

            if line.starts_with(find) {
                let to_replace = Util::str_extract_value(find, line).into();

                if let Ok(Some(next_line)) = lines.next_line().await {
                    let replace_with = Util::str_extract_value(replace, &next_line).into();

                    replace_vec.push(Replace {
                        to_replace,
                        replace_with,
                    });
                }

                continue;
            }
        }

        let config = ConfigEntry {
            xpath_title,
            xpath_author,
            xpath_date,
            xpath_body,
            xpath_strip,
            strip_id_or_class,
            strip_image_src,
            replace: replace_vec,
            header: header_vec,
            single_page_link,
            next_page_link,
        };

        Ok(config)
    }
}
