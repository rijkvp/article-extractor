use std::collections::HashSet;

use once_cell::sync::Lazy;
use regex::Regex;

pub const DEFAULT_CHAR_THRESHOLD: usize = 500;
pub static IS_IMAGE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/\.(jpg|jpeg|png|webp)/i"#).expect("IS_IMAGE regex"));
pub static SIBLING_CONTENT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/\.( |$)/"#).expect("SIBLING_CONTENT regex"));
pub static BYLINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"/byline|author|dateline|writtenby|p-author/i"#).expect("BYLINE regex")
});
pub static NORMALIZE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/\s{2,}/g"#).expect("NORMALIZE regex"));
pub static TOKENIZE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\W+"#).expect("TOKENIZE regex"));
pub static UNLIELY_CANDIDATES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"/-ad-|ai2html|banner|breadcrumbs|combx|comment|community|cover-wrap|disqus|extra|footer|gdpr|header|legends|menu|related|remark|replies|rss|shoutbox|sidebar|skyscraper|social|sponsor|supplemental|ad-break|agegate|pagination|pager|popup|yom-remote/i"#).expect("UNLIELY_CANDIDATES regex")
});
pub static OKAY_MAYBE_ITS_A_CANDIDATE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"/and|article|body|column|content|main|shadow/i"#)
        .expect("OKAY_MAYBE_ITS_A_CANDIDATE regex")
});
pub static HAS_CONTENT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/\S$/"#).expect("HAS_CONTENT regex"));
pub static HASH_URL: Lazy<Regex> = Lazy::new(|| Regex::new(r#"/^#.+/"#).expect("HASH_URL regex"));
pub static POSITIVE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"/article|body|content|entry|hentry|h-entry|main|page|pagination|post|text|blog|story/i"#,
    )
    .expect("POSITIVE regex")
});
pub static NEGATIVE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/-ad-|hidden|^hid$| hid$| hid |^hid"#).expect("NEGATIVE regex"));

pub static TITLE_SEPARATOR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"[-|\\/>»]"#).expect("TITLE_SEPARATOR regex"));
pub static TITLE_CUT_END: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(.*)[-|\\/>»] .*"#).expect("TITLE_CUT_END regex"));
pub static WORD_COUNT: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\s+"#).expect("WORD_COUNT regex"));
pub static TITLE_CUT_FRONT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/[^-|\\/>»]*[-|\\/>»](.*)/gi"#).expect("TITLE_CUT_FRONT regex"));

pub const SCORE_ATTR: &str = "content_score";
pub const MINIMUM_TOPCANDIDATES: usize = 3;
pub const UNLIKELY_ROLES: &[&str] = &[
    "menu",
    "menubar",
    "complementary",
    "navigation",
    "alert",
    "alertdialog",
    "dialog",
];

pub const DEFAULT_TAGS_TO_SCORE: &[&str] =
    &["SECTION", "H2", "H3", "H4", "H5", "H6", "P", "TD", "PRE"];
pub static DIV_TO_P_ELEMS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    HashSet::from([
        "BLOCKQUOTE",
        "DL",
        "DIV",
        "IMG",
        "OL",
        "P",
        "PRE",
        "TABLE",
        "UL",
    ])
});

pub static ALTER_TO_DIV_EXCEPTIONS: Lazy<HashSet<&'static str>> =
    Lazy::new(|| HashSet::from(["DIV", "ARTICLE", "SECTION", "P"]));

pub const PHRASING_ELEMS: &[&str] = &[
    // "CANVAS", "IFRAME", "SVG", "VIDEO",
    "ABBR", "AUDIO", "B", "BDO", "BR", "BUTTON", "CITE", "CODE", "DATA", "DATALIST", "DFN", "EM",
    "EMBED", "I", "IMG", "INPUT", "KBD", "LABEL", "MARK", "MATH", "METER", "NOSCRIPT", "OBJECT",
    "OUTPUT", "PROGRESS", "Q", "RUBY", "SAMP", "SCRIPT", "SELECT", "SMALL", "SPAN", "STRONG",
    "SUB", "SUP", "TEXTAREA", "TIME", "VAR", "WBR",
];
