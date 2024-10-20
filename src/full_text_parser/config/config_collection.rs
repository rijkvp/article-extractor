use rust_embed::RustEmbed;
use std::{borrow::Borrow, collections::HashMap, fs, path::Path};

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
            let entry = match ConfigEntry::parse_data(entry.data) {
                Ok(entry) => entry,
                Err(error) => {
                    log::error!("{error}");
                    continue;
                }
            };
            let file_name: &str = file_name.borrow();
            embedded_entries.insert(file_name.to_owned(), entry);
        }

        if let Some(directory) = directory {
            // create data dir if it doesn't already exist
            if let Err(error) = std::fs::DirBuilder::new().recursive(true).create(directory) {
                log::warn!(
                    "Failed to create user config directory {:?}: {}",
                    directory,
                    error
                );
            }

            if let Ok(mut dir) = fs::read_dir(directory) {
                while let Some(Ok(entry)) = dir.next() {
                    if Util::check_extension(&entry, "txt") {
                        if let Ok(config) = ConfigEntry::parse_path(&entry.path()) {
                            let file_name = entry.file_name().to_string_lossy().into_owned();
                            user_entries.insert(file_name, config);
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
}

#[cfg(test)]
mod tests {
    use super::ConfigCollection;
    use std::path::Path;

    fn read_dir() {
        let path = Path::new("~/.local/share/news-flash/ftr-site-config");
        let _collection = ConfigCollection::parse(Some(path));
    }
}
