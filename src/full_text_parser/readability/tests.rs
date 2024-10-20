use crate::{
    article::Article,
    full_text_parser::{config::ConfigEntry, metadata},
    util::Util,
};
use libxml::tree::{Document, Node};
use url::Url;

fn run_test(name: &str) {
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

    crate::FullTextParser::prep_content(&xpath_ctx, None, &empty_config, &url, &document, None);
    let mut article = Article {
        title: None,
        author: None,
        url,
        date: None,
        thumbnail_url: None,
        html: None,
    };

    let mut article_document = Document::new().unwrap();
    let mut root = Node::new("article", None, &document).unwrap();
    article_document.set_root_element(&root);

    metadata::extract(&xpath_ctx, None, None, &mut article);
    super::Readability::extract_body(document, &mut root, article.title.as_deref()).unwrap();

    let article_ctx = crate::FullTextParser::get_xpath_ctx(&article_document).unwrap();

    crate::FullTextParser::prevent_self_closing_tags(&article_ctx).unwrap();
    crate::FullTextParser::post_process_document(&article_document).unwrap();

    let html = Util::serialize_node(&article_document, &root);

    // abuse line below to update all test results after whitespace or similar change
    // std::fs::write(format!("./resources/tests/readability/{name}/expected.html"), &html).unwrap();

    let expected = std::fs::read_to_string(format!(
        "./resources/tests/readability/{name}/expected.html"
    ))
    .expect("Failed to read expected HTML");

    assert_eq!(expected, html);
}

#[test]
fn test_001() {
    run_test("001")
}

#[test]
fn test_002() {
    run_test("002")
}

#[test]
fn test_003() {
    run_test("003")
}

#[test]
fn aclu() {
    run_test("aclu")
}

#[test]
fn aktualne() {
    run_test("aktualne")
}

#[test]
fn archive_of_our_own() {
    run_test("archive-of-our-own")
}

#[test]
fn ars_1() {
    run_test("ars-1")
}

#[test]
fn base_url_base_element_relative() {
    run_test("base-url-base-element-relative")
}

#[test]
fn basic_tags_cleaning() {
    run_test("basic-tags-cleaning")
}

#[test]
fn bbc_1() {
    run_test("bbc-1")
}

#[test]
fn blogger() {
    run_test("blogger")
}

#[test]
fn breitbart() {
    run_test("breitbart")
}

#[test]
fn bug_1255978() {
    run_test("bug-1255978")
}

#[test]
fn buzzfeed_1() {
    run_test("buzzfeed-1")
}

#[test]
fn citylab_1() {
    run_test("citylab-1")
}

#[test]
fn clean_links() {
    run_test("clean-links")
}

#[test]
fn cnet_svg_classes() {
    run_test("cnet-svg-classes")
}

#[test]
fn cnet() {
    run_test("cnet")
}

#[test]
fn cnn() {
    run_test("cnn")
}

#[test]
fn comment_inside_script_parsing() {
    run_test("comment-inside-script-parsing")
}

#[test]
fn daringfireball_1() {
    run_test("daringfireball-1")
}

#[test]
fn data_url_image() {
    run_test("data-url-image")
}

#[test]
fn dev418() {
    run_test("dev418")
}

#[test]
fn dropbox_blog() {
    run_test("dropbox-blog")
}

#[test]
fn ebb_org() {
    run_test("ebb-org")
}

#[test]
fn ehow_1() {
    run_test("ehow-1")
}

#[test]
fn ehow_2() {
    run_test("ehow-2")
}

#[test]
fn embedded_videos() {
    run_test("embedded-videos")
}

#[test]
fn engadget() {
    run_test("engadget")
}

#[test]
fn firefox_nightly_blog() {
    run_test("firefox-nightly-blog")
}

#[test]
fn folha() {
    run_test("folha")
}

#[test]
fn gmw() {
    run_test("gmw")
}

#[test]
fn google_sre_book_1() {
    run_test("google-sre-book-1")
}

#[test]
fn guardian_1() {
    run_test("guardian-1")
}

#[test]
fn heise() {
    run_test("heise")
}

#[test]
fn herald_sun_1() {
    run_test("herald-sun-1")
}

#[test]
fn hidden_nodes() {
    run_test("hidden-nodes")
}

#[test]
fn hukumusume() {
    run_test("hukumusume")
}

#[test]
fn iab_1() {
    run_test("iab-1")
}

#[test]
fn ietf_1() {
    run_test("ietf-1")
}

#[test]
fn js_link_replacement() {
    run_test("js-link-replacement")
}

#[test]
fn keep_images() {
    run_test("keep-images")
}

#[test]
fn keep_tabular_data() {
    run_test("keep-tabular-data")
}

#[test]
fn la_nacion() {
    run_test("la-nacion")
}

#[test]
fn lazy_image_1() {
    run_test("lazy-image-1")
}

#[test]
fn lazy_image_2() {
    run_test("lazy-image-2")
}

#[test]
fn lazy_image_3() {
    run_test("lazy-image-3")
}

#[test]
fn lemonde_1() {
    run_test("lemonde-1")
}

#[test]
fn liberation_1() {
    run_test("liberation-1")
}

#[test]
fn lifehacker_post_comment_load() {
    run_test("lifehacker-post-comment-load")
}

#[test]
fn lifehacker_working() {
    run_test("lifehacker-working")
}

#[test]
fn links_in_tables() {
    run_test("links-in-tables")
}

#[test]
fn lwn_1() {
    run_test("lwn-1")
}

#[test]
fn medicalnewstoday() {
    run_test("medicalnewstoday")
}

#[test]
fn medium_1() {
    run_test("medium-1")
}

#[test]
fn medium_2() {
    run_test("medium-2")
}

#[test]
fn medium_3() {
    run_test("medium-3")
}

#[test]
fn mercurial() {
    run_test("mercurial")
}

#[test]
fn metadata_content_missing() {
    run_test("metadata-content-missing")
}

#[test]
fn missing_paragraphs() {
    run_test("missing-paragraphs")
}

#[test]
fn mozilla_1() {
    run_test("mozilla-1")
}

#[test]
fn mozilla_2() {
    run_test("mozilla-2")
}

#[test]
fn msn() {
    run_test("msn")
}

#[test]
fn normalize_spaces() {
    run_test("normalize-spaces")
}

#[test]
fn nytimes_1() {
    run_test("nytimes-1")
}

#[test]
fn nytimes_2() {
    run_test("nytimes-2")
}

#[test]
fn nytimes_3() {
    run_test("nytimes-3")
}

#[test]
fn nytimes_4() {
    run_test("nytimes-4")
}

#[test]
fn nytimes_5() {
    run_test("nytimes-5")
}

#[test]
fn pixnet() {
    run_test("pixnet")
}

#[test]
fn qq() {
    run_test("qq")
}

#[test]
fn quanta_1() {
    run_test("quanta-1")
}

#[test]
fn remove_aria_hidden() {
    run_test("remove-aria-hidden")
}

#[test]
fn remove_extra_paragraphs() {
    run_test("remove-extra-paragraphs")
}

#[test]
fn reordering_paragraphs() {
    run_test("reordering-paragraphs")
}

#[test]
fn remove_script_tags() {
    run_test("remove-script-tags")
}

#[test]
fn replace_font_tags() {
    run_test("replace-font-tags")
}

#[test]
fn salon_1() {
    run_test("salon-1")
}

#[test]
fn seattletimes_1() {
    run_test("seattletimes-1")
}

#[test]
fn simplyfound_1() {
    run_test("simplyfound-1")
}

#[test]
fn social_buttons() {
    run_test("social-buttons")
}

#[test]
fn style_tags_removal() {
    run_test("style-tags-removal")
}

#[test]
fn svg_parsing() {
    run_test("svg-parsing")
}

#[test]
fn table_style_attributes() {
    run_test("table-style-attributes")
}

#[test]
fn title_and_h1_discrepancy() {
    run_test("title-and-h1-discrepancy")
}

#[test]
fn tmz_1() {
    run_test("tmz-1")
}

#[test]
fn telegraph() {
    run_test("telegraph")
}

#[test]
fn toc_missing() {
    run_test("toc-missing")
}

#[test]
fn topicseed_1() {
    run_test("topicseed-1")
}

#[test]
fn tumblr() {
    run_test("tumblr")
}

#[test]
fn v8_blog() {
    run_test("v8-blog")
}

#[test]
fn videos_1() {
    run_test("videos-1")
}

#[test]
fn videos_2() {
    run_test("videos-2")
}

#[test]
fn wapo_1() {
    run_test("wapo-1")
}

#[test]
fn wapo_2() {
    run_test("wapo-2")
}

#[test]
fn webmd_1() {
    run_test("webmd-1")
}

#[test]
fn webmd_2() {
    run_test("webmd-2")
}

#[test]
fn wikia() {
    run_test("wikia")
}

#[test]
fn wikipedia() {
    run_test("wikipedia")
}

#[test]
fn wikipedia_2() {
    run_test("wikipedia-2")
}

#[test]
fn wikipedia_3() {
    run_test("wikipedia-3")
}

#[test]
fn wordpress() {
    run_test("wordpress")
}

#[test]
fn yahoo_1() {
    run_test("yahoo-1")
}

#[test]
fn yahoo_2() {
    run_test("yahoo-2")
}

#[test]
fn yahoo_3() {
    run_test("yahoo-3")
}

#[test]
fn yahoo_4() {
    run_test("yahoo-4")
}

#[test]
fn youth() {
    run_test("youth")
}
