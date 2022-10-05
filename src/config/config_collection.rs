use rust_embed::RustEmbed;
use std::{borrow::Borrow, collections::HashMap, path::Path};

use super::ConfigEntry;
use crate::util::Util;

#[derive(RustEmbed)]
#[folder = "ftr-site-config"]
struct EmbededConfigFiles;

pub struct ConfigCollection {
    embedded_entries: HashMap<String, ConfigEntry>,
    user_entries: HashMap<String, ConfigEntry>,
}

impl ConfigCollection {
    pub async fn parse(directory: Option<&Path>) -> ConfigCollection {

        let mut user_entries = HashMap::new();
        let mut embedded_entries = HashMap::new();

        for (file_name, entry) in EmbededConfigFiles::iter()
            .filter_map(|file_name| EmbededConfigFiles::get(&file_name).map(|e| (file_name, e)))
        {
            if let Ok(entry) = ConfigEntry::parse_data(entry.data).await {
                let file_name: &str = file_name.borrow();
                embedded_entries.insert(file_name.to_owned(), entry);
            }
        }

        if let Some(directory) = directory {
            // create data dir if it doesn't already exist
            if let Err(error) = std::fs::DirBuilder::new()
                .recursive(true)
                .create(&directory)
            {
                log::warn!(
                    "Failed to create user config directory {:?}: {}",
                    directory,
                    error
                );
            }

            if let Ok(mut dir) = tokio::fs::read_dir(directory).await {
                while let Ok(entry) = dir.next_entry().await {
                    if let Some(entry) = entry {
                        if Util::check_extension(&entry, "txt") {
                            if let Ok(config) = ConfigEntry::parse_path(&entry.path()).await {
                                let file_name = entry.file_name().to_string_lossy().into_owned();
                                user_entries.insert(file_name, config);
                            }
                        }
                    }
                }
            }
        }

        Self {
            embedded_entries,
            user_entries,
        }
    }

    pub fn get(&self, key: &str) -> Option<&ConfigEntry> {
        if let Some(user_entry) = self.user_entries.get(key) {
            Some(user_entry)
        } else {
            self.embedded_entries.get(key)
        }
    }

    pub fn contains_config(&self, key: &str) -> bool {
        self.user_entries.contains_key(key) || self.embedded_entries.contains_key(key)
    }
}
