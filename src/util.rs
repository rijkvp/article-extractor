use failure::ResultExt;
use reqwest::header::{HeaderMap, HeaderValue, HeaderName};
use tokio::fs::DirEntry;

use crate::{config::ConfigEntry, error::{ScraperErrorKind, ScraperError}};

pub struct Util;

impl Util {
    pub fn check_extension(path: &DirEntry, extension: &str) -> bool {
        if let Some(ext) = path.path().extension() {
            ext.to_str() == Some(extension)
        } else {
            false
        }
    }

    pub fn extract_value<'a>(identifier: &str, line: &'a str) -> &'a str {
        let value = &line[identifier.len()..];
        let value = value.trim();
        match value.find('#') {
            Some(pos) => &value[..pos],
            None => value,
        }
    }

    pub fn split_values(values: &str) -> Vec<&str> {
        values.split('|').map(|s| s.trim()).collect()
    }

    pub fn select_rule<'a>(
        site_specific_rule: Option<&'a str>,
        global_rule: Option<&'a str>,
    ) -> Option<&'a str> {
        if site_specific_rule.is_some() {
            site_specific_rule
        } else {
            global_rule
        }
    }

    pub fn generate_headers(site_specific_rule: Option<&ConfigEntry>, global_rule: &ConfigEntry) -> Result<HeaderMap, ScraperError> {
        let mut headers = HeaderMap::new();

        if let Some(config) = site_specific_rule {
            for header in &config.header {
                let name = HeaderName::from_bytes(header.name.as_bytes()).context(ScraperErrorKind::Config)?;
                let value = header.value.parse::<HeaderValue>().context(ScraperErrorKind::Config)?;
                headers.insert(name, value);
            }
        }

        for header in &global_rule.header {
            let name = HeaderName::from_bytes(header.name.as_bytes()).context(ScraperErrorKind::Config)?;
            let value = header.value.parse::<HeaderValue>().context(ScraperErrorKind::Config)?;
            headers.insert(name, value);
        }

        Ok(headers)
    }
}
