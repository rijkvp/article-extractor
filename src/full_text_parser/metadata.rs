use super::config::ConfigEntry;
use crate::{article::Article, constants, util::Util};
use chrono::{DateTime, Utc};
use libxml::xpath::Context;
use log::{debug, warn};
use std::str::FromStr;

pub fn extract(
    context: &Context,
    config: Option<&ConfigEntry>,
    global_config: Option<&ConfigEntry>,
    article: &mut Article,
) {
    if article.title.is_none() {
        article.title = extract_title(context, config, global_config)
            .map(|title| match escaper::decode_html(&title) {
                Ok(escaped_title) => escaped_title,
                Err(_error) => title,
            })
            .map(|title| {
                // clean titles that contain separators
                if constants::TITLE_SEPARATOR.is_match(&title) {
                    let new_title = constants::TITLE_CUT_END.replace(&title, "$1");
                    let word_count = constants::WORD_COUNT.split(&title).count();
                    if word_count < 3 {
                        constants::TITLE_CUT_FRONT.replace(&title, "$1").to_string()
                    } else {
                        new_title.to_string()
                    }
                } else {
                    title
                }
            });
    }

    if article.author.is_none() {
        article.author =
            extract_author(context, config, global_config).map(
                |author| match escaper::decode_html(&author) {
                    Ok(escaped_author) => escaped_author,
                    Err(_error) => author,
                },
            );
    }

    if article.date.is_none() {
        article.date = extract_date(context, config, global_config);
    }
}

fn extract_title(
    context: &Context,
    config: Option<&ConfigEntry>,
    global_config: Option<&ConfigEntry>,
) -> Option<String> {
    // check site specific config
    if let Some(config) = config {
        for xpath_title in &config.xpath_title {
            if let Ok(title) = Util::extract_value_merge(context, xpath_title) {
                debug!("Article title: '{}'", title);
                return Some(title);
            }
        }
    }

    // check global config
    if let Some(global_config) = global_config {
        for xpath_title in &global_config.xpath_title {
            if let Ok(title) = Util::extract_value_merge(context, xpath_title) {
                debug!("Article title: '{}'", title);
                return Some(title);
            }
        }
    }

    // generic meta (readablity)
    Util::extract_value(context, "//title")
        .ok()
        .or_else(|| get_meta(context, "dc:title"))
        .or_else(|| get_meta(context, "dcterm:title"))
        .or_else(|| get_meta(context, "og:title"))
        .or_else(|| get_meta(context, "weibo:article:title"))
        .or_else(|| get_meta(context, "weibo:webpage:title"))
        .or_else(|| get_meta(context, "twitter:title"))
}

fn extract_author(
    context: &Context,
    config: Option<&ConfigEntry>,
    global_config: Option<&ConfigEntry>,
) -> Option<String> {
    // check site specific config
    if let Some(config) = config {
        for xpath_author in &config.xpath_author {
            if let Ok(author) = Util::extract_value(context, xpath_author) {
                debug!("Article author: '{}'", author);
                return Some(author);
            }
        }
    }

    // check global config
    if let Some(global_config) = global_config {
        for xpath_author in &global_config.xpath_author {
            if let Ok(author) = Util::extract_value(context, xpath_author) {
                debug!("Article author: '{}'", author);
                return Some(author);
            }
        }
    }

    // generic meta (readablity)
    Util::extract_value(context, "//author")
        .ok()
        .or_else(|| get_meta(context, "dc:creator"))
        .or_else(|| get_meta(context, "dcterm:creator"))
}

fn extract_date(
    context: &Context,
    config: Option<&ConfigEntry>,
    global_config: Option<&ConfigEntry>,
) -> Option<DateTime<Utc>> {
    // check site specific config
    if let Some(config) = config {
        for xpath_date in &config.xpath_date {
            if let Ok(date_string) = Util::extract_value(context, xpath_date) {
                debug!("Article date: '{}'", date_string);
                if let Ok(date) = DateTime::from_str(&date_string) {
                    return Some(date);
                } else {
                    warn!("Parsing the date string '{}' failed", date_string);
                }
            }
        }
    }

    // check global config
    if let Some(global_config) = global_config {
        for xpath_date in &global_config.xpath_date {
            if let Ok(date_string) = Util::extract_value(context, xpath_date) {
                debug!("Article date: '{}'", date_string);
                if let Ok(date) = DateTime::from_str(&date_string) {
                    return Some(date);
                } else {
                    warn!("Parsing the date string '{}' failed", date_string);
                }
            }
        }
    }

    None
}

fn get_meta(context: &Context, name: &str) -> Option<String> {
    Util::get_attribute(
        context,
        &format!("//meta[contains(@name, '{}')]", name),
        "content",
    )
    .ok()
}
