use libxml::tree::{Document, Node};
use reqwest::Url;

use crate::{
    article::Article,
    full_text_parser::{config::ConfigEntry, metadata},
};

async fn run_test(name: &str) {
    libxml::tree::node::set_node_rc_guard(10);
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .try_init();

    let empty_config = ConfigEntry::default();

    let url = Url::parse("http://fakehost/test/base/").unwrap();
    let html = std::fs::read_to_string(format!("./resources/tests/readability/{name}/source.html"))
        .expect("Failed to read source HTML");

    let document = crate::FullTextParser::parse_html(&html, None, &empty_config).unwrap();
    let xpath_ctx = crate::FullTextParser::get_xpath_ctx(&document).unwrap();

    crate::FullTextParser::prep_content(&xpath_ctx, None, &empty_config, &url, &document);
    let mut article = Article {
        title: None,
        author: None,
        url,
        date: None,
        thumbnail_url: None,
        document: None,
        root_node: None,
    };

    let mut article_document = Document::new().unwrap();
    let mut root = Node::new("article", None, &document).unwrap();
    article_document.set_root_element(&root);

    metadata::extract(&xpath_ctx, None, None, &mut article);
    super::Readability::extract_body(document, &mut root, article.title.as_deref()).unwrap();
    crate::FullTextParser::post_process_document(&article_document).unwrap();

    article.document = Some(article_document);
    article.root_node = Some(root);
    let html = article.get_content().unwrap();

    //std::fs::write(format!("./resources/tests/readability/{name}/expected.html"), &html).unwrap();

    let expected = std::fs::read_to_string(format!(
        "./resources/tests/readability/{name}/expected.html"
    ))
    .expect("Failed to read expected HTML");

    assert_eq!(expected, html);
}

#[tokio::test]
async fn test_001() {
    run_test("001").await
}

#[tokio::test]
async fn test_002() {
    run_test("002").await
}

#[tokio::test]
async fn test_003() {
    run_test("003").await
}

#[tokio::test]
async fn aclu() {
    run_test("aclu").await
}

#[tokio::test]
async fn aktualne() {
    run_test("aktualne").await
}

#[tokio::test]
async fn archive_of_our_own() {
    run_test("archive-of-our-own").await
}

#[tokio::test]
async fn ars_1() {
    run_test("ars-1").await
}

#[tokio::test]
async fn base_url_base_element_relative() {
    run_test("base-url-base-element-relative").await
}

#[tokio::test]
async fn basic_tags_cleaning() {
    run_test("basic-tags-cleaning").await
}

#[tokio::test]
async fn bbc_1() {
    run_test("bbc-1").await
}

#[tokio::test]
async fn blogger() {
    run_test("blogger").await
}

#[tokio::test]
async fn breitbart() {
    run_test("breitbart").await
}

#[tokio::test]
async fn bug_1255978() {
    run_test("bug-1255978").await
}

#[tokio::test]
async fn buzzfeed_1() {
    run_test("buzzfeed-1").await
}

#[tokio::test]
async fn citylab_1() {
    run_test("citylab-1").await
}

#[tokio::test]
async fn clean_links() {
    run_test("clean-links").await
}

#[tokio::test]
async fn cnet_svg_classes() {
    run_test("cnet-svg-classes").await
}

#[tokio::test]
async fn cnet() {
    run_test("cnet").await
}

#[tokio::test]
async fn cnn() {
    run_test("cnn").await
}

#[tokio::test]
async fn comment_inside_script_parsing() {
    run_test("comment-inside-script-parsing").await
}

#[tokio::test]
async fn daringfireball_1() {
    run_test("daringfireball-1").await
}

#[tokio::test]
async fn data_url_image() {
    run_test("data-url-image").await
}

#[tokio::test]
async fn dev418() {
    run_test("dev418").await
}

#[tokio::test]
async fn dropbox_blog() {
    run_test("dropbox-blog").await
}

#[tokio::test]
async fn ebb_org() {
    run_test("ebb-org").await
}

#[tokio::test]
async fn ehow_1() {
    run_test("ehow-1").await
}

#[tokio::test]
async fn ehow_2() {
    run_test("ehow-2").await
}

#[tokio::test]
async fn embedded_videos() {
    run_test("embedded-videos").await
}

#[tokio::test]
async fn engadget() {
    run_test("engadget").await
}

#[tokio::test]
async fn firefox_nightly_blog() {
    run_test("firefox-nightly-blog").await
}

#[tokio::test]
async fn folha() {
    run_test("folha").await
}

#[tokio::test]
async fn gmw() {
    run_test("gmw").await
}

#[tokio::test]
async fn google_sre_book_1() {
    run_test("google-sre-book-1").await
}

#[tokio::test]
async fn guardian_1() {
    run_test("guardian-1").await
}

#[tokio::test]
async fn heise() {
    run_test("heise").await
}

#[tokio::test]
async fn herald_sun_1() {
    run_test("herald-sun-1").await
}

#[tokio::test]
async fn hidden_nodes() {
    run_test("hidden-nodes").await
}

#[tokio::test]
async fn hukumusume() {
    run_test("hukumusume").await
}

#[tokio::test]
async fn iab_1() {
    run_test("iab-1").await
}

#[tokio::test]
async fn ietf_1() {
    run_test("ietf-1").await
}

#[tokio::test]
async fn js_link_replacement() {
    run_test("js-link-replacement").await
}

#[tokio::test]
async fn keep_images() {
    run_test("keep-images").await
}

#[tokio::test]
async fn keep_tabular_data() {
    run_test("keep-tabular-data").await
}

#[tokio::test]
async fn la_nacion() {
    run_test("la-nacion").await
}

#[tokio::test]
async fn lazy_image_1() {
    run_test("lazy-image-1").await
}

#[tokio::test]
async fn lazy_image_2() {
    run_test("lazy-image-2").await
}

#[tokio::test]
async fn lazy_image_3() {
    run_test("lazy-image-3").await
}

#[tokio::test]
async fn lemonde_1() {
    run_test("lemonde-1").await
}

#[tokio::test]
async fn liberation_1() {
    run_test("liberation-1").await
}

#[tokio::test]
async fn lifehacker_post_comment_load() {
    run_test("lifehacker-post-comment-load").await
}

#[tokio::test]
async fn lifehacker_working() {
    run_test("lifehacker-working").await
}

#[tokio::test]
async fn links_in_tables() {
    run_test("links-in-tables").await
}

#[tokio::test]
async fn lwn_1() {
    run_test("lwn-1").await
}

#[tokio::test]
async fn medicalnewstoday() {
    run_test("medicalnewstoday").await
}

#[tokio::test]
async fn medium_1() {
    run_test("medium-1").await
}

#[tokio::test]
async fn medium_2() {
    run_test("medium-2").await
}

#[tokio::test]
async fn medium_3() {
    run_test("medium-3").await
}

#[tokio::test]
async fn mercurial() {
    run_test("mercurial").await
}

#[tokio::test]
async fn metadata_content_missing() {
    run_test("metadata-content-missing").await
}

#[tokio::test]
async fn missing_paragraphs() {
    run_test("missing-paragraphs").await
}

#[tokio::test]
async fn mozilla_1() {
    run_test("mozilla-1").await
}

#[tokio::test]
async fn mozilla_2() {
    run_test("mozilla-2").await
}

#[tokio::test]
async fn msn() {
    run_test("msn").await
}

#[tokio::test]
async fn normalize_spaces() {
    run_test("normalize-spaces").await
}

#[tokio::test]
async fn nytimes_1() {
    run_test("nytimes-1").await
}

#[tokio::test]
async fn nytimes_2() {
    run_test("nytimes-2").await
}

#[tokio::test]
async fn nytimes_3() {
    run_test("nytimes-3").await
}

#[tokio::test]
async fn nytimes_4() {
    run_test("nytimes-4").await
}

#[tokio::test]
async fn nytimes_5() {
    run_test("nytimes-5").await
}

#[tokio::test]
async fn pixnet() {
    run_test("pixnet").await
}

#[tokio::test]
async fn qq() {
    run_test("qq").await
}

#[tokio::test]
async fn quanta_1() {
    run_test("quanta-1").await
}

#[tokio::test]
async fn remove_aria_hidden() {
    run_test("remove-aria-hidden").await
}

#[tokio::test]
async fn remove_extra_paragraphs() {
    run_test("remove-extra-paragraphs").await
}

#[tokio::test]
async fn reordering_paragraphs() {
    run_test("reordering-paragraphs").await
}

#[tokio::test]
async fn remove_script_tags() {
    run_test("remove-script-tags").await
}

#[tokio::test]
async fn replace_font_tags() {
    run_test("replace-font-tags").await
}

#[tokio::test]
async fn salon_1() {
    run_test("salon-1").await
}

#[tokio::test]
async fn seattletimes_1() {
    run_test("seattletimes-1").await
}

#[tokio::test]
async fn simplyfound_1() {
    run_test("simplyfound-1").await
}

#[tokio::test]
async fn social_buttons() {
    run_test("social-buttons").await
}

#[tokio::test]
async fn style_tags_removal() {
    run_test("style-tags-removal").await
}

#[tokio::test]
async fn svg_parsing() {
    run_test("svg-parsing").await
}

#[tokio::test]
async fn table_style_attributes() {
    run_test("table-style-attributes").await
}

#[tokio::test]
async fn title_and_h1_discrepancy() {
    run_test("title-and-h1-discrepancy").await
}

#[tokio::test]
async fn tmz_1() {
    run_test("tmz-1").await
}

#[tokio::test]
async fn telegraph() {
    run_test("telegraph").await
}

#[tokio::test]
async fn toc_missing() {
    run_test("toc-missing").await
}

#[tokio::test]
async fn topicseed_1() {
    run_test("topicseed-1").await
}

#[tokio::test]
async fn tumblr() {
    run_test("tumblr").await
}

#[tokio::test]
async fn v8_blog() {
    run_test("v8-blog").await
}

#[tokio::test]
async fn videos_1() {
    run_test("videos-1").await
}

#[tokio::test]
async fn videos_2() {
    run_test("videos-2").await
}

#[tokio::test]
async fn wapo_1() {
    run_test("wapo-1").await
}

#[tokio::test]
async fn wapo_2() {
    run_test("wapo-2").await
}

#[tokio::test]
async fn webmd_1() {
    run_test("webmd-1").await
}

#[tokio::test]
async fn webmd_2() {
    run_test("webmd-2").await
}

#[tokio::test]
async fn wikia() {
    run_test("wikia").await
}

#[tokio::test]
async fn wikipedia() {
    run_test("wikipedia").await
}

#[tokio::test]
async fn wikipedia_2() {
    run_test("wikipedia-2").await
}

#[tokio::test]
async fn wikipedia_3() {
    run_test("wikipedia-3").await
}

#[tokio::test]
async fn wordpress() {
    run_test("wordpress").await
}

#[tokio::test]
async fn yahoo_1() {
    run_test("yahoo-1").await
}

#[tokio::test]
async fn yahoo_2() {
    run_test("yahoo-2").await
}

#[tokio::test]
async fn yahoo_3() {
    run_test("yahoo-3").await
}

#[tokio::test]
async fn yahoo_4() {
    run_test("yahoo-4").await
}

#[tokio::test]
async fn youth() {
    run_test("youth").await
}
