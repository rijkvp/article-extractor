use libxml::tree::{Document, Node};
use reqwest::Url;

use crate::{
    article::Article,
    full_text_parser::{config::ConfigEntry, metadata},
};

async fn run_test(name: &str) {
    libxml::tree::node::set_node_rc_guard(3);
    let _ = env_logger::builder().is_test(true).try_init();

    let empty_config = ConfigEntry::default();

    let url = Url::parse("http://google.com").unwrap();
    let html = std::fs::read_to_string(format!("./resources/tests/readability/{name}/source.html"))
        .expect("Failed to read source HTML");
    let document = crate::FullTextParser::parse_html(&html, None, &empty_config).unwrap();
    let xpath_ctx = crate::FullTextParser::get_xpath_ctx(&document).unwrap();

    crate::FullTextParser::strip_junk(&xpath_ctx, None, &empty_config);
    crate::FullTextParser::unwrap_noscript_images(&xpath_ctx).unwrap();
    let mut article = Article {
        title: None,
        author: None,
        url: url.clone(),
        date: None,
        thumbnail_url: None,
        document: None,
    };

    let mut article_document = Document::new().unwrap();
    let mut root = Node::new("article", None, &document).unwrap();
    article_document.set_root_element(&root);

    metadata::extract(&xpath_ctx, None, None, &mut article);
    super::Readability::extract_body(document, &mut root, article.title.as_deref()).unwrap();

    if let Some(mut root) = article_document.get_root_element() {
        crate::FullTextParser::post_process_content(&mut root).unwrap();
    }

    article.document = Some(article_document);
    let html = article.get_content().unwrap();

    let expected = std::fs::read_to_string(format!(
        "./resources/tests/readability/{name}/expected.html"
    ))
    .expect("Failed to read expected HTML");

    //std::fs::write("expected.html", &html).unwrap();

    assert_eq!(expected, html);
}

#[tokio::test(flavor = "current_thread")]
async fn test_001() {
    run_test("001").await
}

#[tokio::test(flavor = "current_thread")]
async fn test_002() {
    run_test("002").await
}
