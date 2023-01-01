use once_cell::sync::Lazy;
use regex::Regex;

pub static BYLINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"/byline|author|dateline|writtenby|p-author/i"#).expect("BYLINE regex")
});
pub static NORMALIZE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"/\s{2,}/g"#).expect("NORMALIZE regex")
});
pub static TOKENIZE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"/\W+/g"#).expect("TOKENIZE regex")
});