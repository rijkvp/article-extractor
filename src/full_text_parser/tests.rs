use super::FullTextParser;
use reqwest::Client;
use std::path::PathBuf;

#[tokio::test]
async fn golem() {
    let out_path = PathBuf::from(r"./test_output");
    let url = url::Url::parse("https://www.golem.de/news/http-error-418-fehlercode-ich-bin-eine-teekanne-darf-bleiben-1708-129460.html").unwrap();

    let grabber = FullTextParser::new(None).await;
    let article = grabber.parse(&url, &Client::new()).await.unwrap();
    article.save_html(&out_path).unwrap();

    assert_eq!(
        article.title,
        Some(String::from(
            "HTTP Error 418: Fehlercode \"Ich bin eine Teekanne\" darf bleiben"
        ))
    );
    assert_eq!(
        article.thumbnail_url,
        Some(String::from(
            "https://www.golem.de/1708/129460-144318-i_rc.jpg"
        ))
    );
    assert_eq!(article.author, Some(String::from("Hauke Gierow")));
}

#[tokio::test]
async fn phoronix() {
    let out_path = PathBuf::from(r"./test_output");
    let url =
        url::Url::parse("http://www.phoronix.com/scan.php?page=article&item=amazon_ec2_bare&num=1")
            .unwrap();

    let grabber = FullTextParser::new(None).await;
    let article = grabber.parse(&url, &Client::new()).await.unwrap();
    article.save_html(&out_path).unwrap();

    assert_eq!(
        article.title,
        Some(String::from(
            "Amazon EC2 Cloud Benchmarks Against Bare Metal Systems"
        ))
    );
}

#[tokio::test]
async fn youtube() {
    let out_path = PathBuf::from(r"./test_output");
    let url = url::Url::parse("https://www.youtube.com/watch?v=8KjaIumu-jI").unwrap();

    let grabber = FullTextParser::new(None).await;
    let article = grabber.parse(&url, &Client::new()).await.unwrap();
    article.save_html(&out_path).unwrap();

    assert_eq!(
        article.title.as_deref(),
        Some("RIGGED! Arena Shuffler is BROKEN | 13 Land Mono Red Burn")
    );
    assert!(article
        .get_content()
        .map(|html| html.contains("https://www.youtube.com/embed/8KjaIumu-jI?feature=oembed"))
        .unwrap_or(false));
}

#[tokio::test]
async fn encoding_windows_1252() {
    let url = url::Url::parse("https://www.aerzteblatt.de/nachrichten/139511/Scholz-zuversichtlich-mit-Blick-auf-Coronasituation-im-Winter").unwrap();
    let html = FullTextParser::download(&url, &Client::new(), reqwest::header::HeaderMap::new())
        .await
        .unwrap();
    assert!(html.contains("Bund-Länder-Konferenz"));
}
