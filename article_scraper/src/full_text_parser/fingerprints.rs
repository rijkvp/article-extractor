use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Url;
use std::collections::HashMap;

static FINGERPRINT_REGEXES: Lazy<HashMap<&'static str, Regex>> = Lazy::new(|| {
    let mut m = HashMap::with_capacity(4);
    m.insert(
        "fingerprint.blogspot.com",
        Regex::new(
            r#"/\\<meta\s*content=([\\'"])blogger([\\'"])\s*name=([\\'"])generator([\\'"])/i"#,
        )
        .expect("failed to build static regex"),
    );
    m.insert(
        "fingerprint.blogspot.com",
        Regex::new(
            r#"/\\<meta\s*name=([\\'"])generator([\\'"])\s*content=([\\'"])Blogger([\\'"])/i"#,
        )
        .expect("failed to build static regex"),
    );
    m.insert(
        "fingerprint.wordpress.com",
        Regex::new(r#"/\\<meta\\s*name=([\\'"])generator([\\'"])\s*content=([\\'"])WordPress/i"#)
            .expect("failed to build static regex"),
    );
    m.insert(
        "fingerprint.ippen.media",
        Regex::new(r#"/\\<div\\s*class=([\\'"])id-SiteBEEPWrap([\\'"])\\>/i"#)
            .expect("failed to build static regex"),
    );
    m
});

pub struct Fingerprints;

impl Fingerprints {
    pub fn detect(html: &str) -> Option<Url> {
        for (url, regex) in FINGERPRINT_REGEXES.iter() {
            if regex.captures(html).is_some() {
                return Some(Url::parse(url).expect("failed to parse static url"));
            }
        }

        None
    }
}
