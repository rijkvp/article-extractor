use std::collections;
use std::path::{PathBuf};
use std::fs;
use std::io;
use std::io::BufRead;
use failure::ResultExt;
use self::error::{ConfigError, ConfigErrorKind};

#[macro_use]
mod macros;
mod error;

pub type ConfigCollection = collections::HashMap<String, GrabberConfig>;

pub struct Replace {
    pub to_replace: String,
    pub replace_with: String,
}

pub struct GrabberConfig {
    pub xpath_title: Vec<String>,
    pub xpath_author: Vec<String>,
    pub xpath_date: Vec<String>,
    pub xpath_body: Vec<String>,
    pub xpath_strip: Vec<String>,
    pub strip_id_or_class: Vec<String>,
    pub strip_image_src: Vec<String>,
    pub replace: Vec<Replace>,
    pub single_page_link: Option<String>,
    pub next_page_link: Option<String>,
}

impl GrabberConfig {

    pub fn parse_directory(directory: &PathBuf) -> Result<ConfigCollection, ConfigError> {
        let paths = fs::read_dir(directory).context(ConfigErrorKind::IO)?;

		let mut collection: collections::HashMap<String, GrabberConfig> = collections::HashMap::new();

        for path in paths {
            if let Ok(path) = path {
                if let Some(extension) = path.path().extension() {
                   if let Some(extension) = extension.to_str() {
                       if extension == "txt" {
                            if let Ok(config) = GrabberConfig::new(path.path()) {
                                collection.insert(path.file_name().to_string_lossy().into_owned(), config);
                            }
                       }
                   } 
                }
            }
        }

		Ok(collection)
    }

    fn new(config_path: PathBuf) -> Result<GrabberConfig, ConfigError> {
        let file = fs::File::open(&config_path).context(ConfigErrorKind::IO)?;
        let buffer = io::BufReader::new(&file);

        let mut xpath_title: Vec<String> = Vec::new();
        let mut xpath_author: Vec<String> = Vec::new();
        let mut xpath_date: Vec<String> = Vec::new();
        let mut xpath_body: Vec<String> = Vec::new();
        let mut xpath_strip: Vec<String> = Vec::new();
        let mut strip_id_or_class: Vec<String> = Vec::new();
        let mut strip_image_src: Vec<String> = Vec::new();
        let mut replace_vec: Vec<Replace> = Vec::new();
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

        // ignore these
        let tidy = "tidy:";
        let prune = "prune:";
        let test_url = "test_url:";
        let autodetect = "autodetect_on_failure:";

        let mut iterator = buffer.lines().peekable();
        while let Some(Ok(line)) = iterator.next() {
            let line = line.trim();
            if line.starts_with("#") 
            || line.starts_with(tidy)
            || line.starts_with(prune)
            || line.starts_with(test_url)
            || line.starts_with(autodetect)
            || line.is_empty() {
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
                let value = GrabberConfig::extract_value(replace_single, line);
                let value: Vec<&str> = value.split("): ").map(|s| s.trim()).collect();
                if value.len() != 2{
                    continue;
                }

                if let Some(to_replace) = value.get(0) {
                    if let Some(replace_with) = value.get(1) {
                        replace_vec.push( 
                            Replace {
                                to_replace: to_replace.to_string(),
                                replace_with: replace_with.to_string(),
                            }
                        );
                    }
                }

				continue;
            }

            if line.starts_with(find) {
                let value1 = GrabberConfig::extract_value(find, line);

                if let Some(&Ok(ref next_line)) = iterator.peek() {
                    let value2 = GrabberConfig::extract_value(replace, &next_line);

                    let r = Replace {
                        to_replace: value1.to_string(),
                        replace_with: value2.to_string(),
                    };

                    replace_vec.push(r);
                }
				continue;
            }
        }

        if xpath_body.len() == 0 {
            error!("No body xpath found for {}", config_path.display());
            Err(ConfigErrorKind::BadConfig)?
        }

        let config = GrabberConfig {
            xpath_title: xpath_title,
            xpath_author: xpath_author,
            xpath_date: xpath_date,
            xpath_body: xpath_body,
            xpath_strip: xpath_strip,
            strip_id_or_class: strip_id_or_class,
            strip_image_src: strip_image_src,
            replace: replace_vec,
            single_page_link: single_page_link,
            next_page_link: next_page_link,
        };

        Ok(config)
    }

    fn extract_value<'a>(identifier: &str, line: &'a str) -> &'a str {
        let value = &line[identifier.len()..];
        let value = value.trim();
        match value.find('#') {
            Some(pos) => &value[..pos],
            None => value,
        }
    }

    fn split_values<'a>(values: &'a str) -> Vec<&'a str> {
        values.split('|').map(|s| s.trim()).collect()
    }
}
