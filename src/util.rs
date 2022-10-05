use tokio::fs::DirEntry;

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
}