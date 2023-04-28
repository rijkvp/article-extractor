use super::{config::ConfigEntry, FullTextParser};
use libxml::tree::SaveOptions;
use reqwest::{Client, Url};

async fn run_test(name: &str, url: &str, title: Option<&str>, author: Option<&str>) {
    libxml::tree::node::set_node_rc_guard(10);
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .try_init();

    let url = Url::parse(url).unwrap();
    let html = std::fs::read_to_string(format!("./resources/tests/ftr/{name}/source.html"))
        .expect("Failed to read source HTML");

    let parser = FullTextParser::new(None).await;
    let article = parser.parse_offline(&html, None, Some(url)).await.unwrap();

    let content = article.get_content().unwrap();

    // abuse line below to update all test results after whitespace or similar change
    // std::fs::write(format!("./resources/tests/ftr/{name}/expected.html"), &content).unwrap();

    let expected = std::fs::read_to_string(format!("./resources/tests/ftr/{name}/expected.html"))
        .expect("Failed to read expected HTML");

    assert_eq!(expected, content);

    if let Some(expected_title) = title {
        assert_eq!(expected_title, article.title.unwrap().as_str());
    }

    if let Some(expected_author) = author {
        assert_eq!(expected_author, article.author.unwrap().as_str());
    }
}

#[tokio::test]
async fn golem() {
    run_test(
        "golem",
        "https://www.golem.de/",
        Some("HTTP Error 418: Fehlercode \"Ich bin eine Teekanne\" darf bleiben"),
        Some("Hauke Gierow"),
    )
    .await
}

#[tokio::test]
async fn phoronix() {
    run_test(
        "phoronix",
        "https://www.phoronix.com/",
        Some("GNOME 44.1 Released With Many Fixes"),
        Some("Michael Larabel"),
    )
    .await
}

#[tokio::test]
async fn youtube() {
    run_test(
        "youtube",
        "https://www.youtube.com/",
        Some("RIGGED! Arena Shuffler is BROKEN"),
        None,
    )
    .await
}

#[tokio::test]
async fn hardwareluxx() {
    run_test("hardwareluxx", "https://www.hardwareluxx.de/", None, None).await
}

#[tokio::test]
#[ignore = "waiting on clarification for https://github.com/fivefilters/ftr-site-config/pull/1081"]
async fn heise_1() {
    run_test("heise-1", "https://www.heise.de/", None, None).await
}

#[tokio::test]
#[ignore = "downloads content from the web"]
async fn encoding_windows_1252() {
    let _ = env_logger::builder().is_test(true).try_init();
    let url = url::Url::parse("https://www.aerzteblatt.de/nachrichten/139511/Scholz-zuversichtlich-mit-Blick-auf-Coronasituation-im-Winter").unwrap();
    let html = FullTextParser::download(&url, &Client::new(), reqwest::header::HeaderMap::new())
        .await
        .unwrap();
    assert!(html.contains("Bund-Länder-Konferenz"));
}

#[tokio::test]
async fn unwrap_noscript_images() {
    let _ = env_logger::builder().is_test(true).try_init();

    let html = r#"
<p>Lorem ipsum dolor sit amet,
    <span class="lazyload">
            <img src="foto-m0101.jpg" alt="image description">
            <noscript><img src="foto-m0102.jpg" alt="image description"></noscript>
    </span>
    consectetur adipiscing elit.
</p>
    "#;

    let expected = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body>
<p>Lorem ipsum dolor sit amet,
    <span class="lazyload">
            <img src="foto-m0102.jpg" alt="image description" data-old-src="foto-m0101.jpg">
            
    </span>
    consectetur adipiscing elit.
</p>
    </body></html>
"#;

    let empty_config = ConfigEntry::default();
    let document = crate::FullTextParser::parse_html(html, None, &empty_config).unwrap();
    let xpath_ctx = crate::FullTextParser::get_xpath_ctx(&document).unwrap();

    crate::FullTextParser::unwrap_noscript_images(&xpath_ctx).unwrap();

    let options = SaveOptions {
        format: true,
        no_declaration: false,
        no_empty_tags: true,
        no_xhtml: false,
        xhtml: false,
        as_xml: false,
        as_html: true,
        non_significant_whitespace: false,
    };
    let res = document.to_string_with_options(options);
    assert_eq!(res, expected);
}

#[tokio::test]
async fn unwrap_noscript_images_2() {
    let _ = env_logger::builder().is_test(true).try_init();

    let html = r#"
<picture class="c-lead-image__image">
    <source srcset="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448" media="(max-width: 575px)" />
    <img class="c-lead-image__img" srcset="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448" alt="" itemprop="contentUrl" onload="performance.mark(&quot;citylab_lead_image_loaded&quot;)" />
    <noscript>
        <img class="c-lead-image__img" src="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448" alt="" />
    </noscript>
</picture>
    "#;

    let expected = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body>
<picture class="c-lead-image__image">
    <source srcset="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448" media="(max-width: 575px)"></source>
    <img class="c-lead-image__img" src="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448" alt="" srcset="https://cdn.citylab.com/media/img/citylab/2019/04/mr1/300.jpg?mod=1556645448">
    
</picture>
    </body></html>
"#;

    let empty_config = ConfigEntry::default();
    let document = crate::FullTextParser::parse_html(html, None, &empty_config).unwrap();
    let xpath_ctx = crate::FullTextParser::get_xpath_ctx(&document).unwrap();

    crate::FullTextParser::unwrap_noscript_images(&xpath_ctx).unwrap();

    let options = SaveOptions {
        format: true,
        no_declaration: false,
        no_empty_tags: true,
        no_xhtml: false,
        xhtml: false,
        as_xml: false,
        as_html: true,
        non_significant_whitespace: false,
    };
    let res = document.to_string_with_options(options);

    assert_eq!(res, expected);
}
