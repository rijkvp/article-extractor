use crate::*;
use reqwest::Client;
use std::path::PathBuf;

#[tokio::test(flavor = "current_thread")]
async fn golem() {
    let out_path = PathBuf::from(r"./test_output");
    let url = url::Url::parse("https://www.golem.de/news/http-error-418-fehlercode-ich-bin-eine-teekanne-darf-bleiben-1708-129460.html").unwrap();

    let grabber = ArticleScraper::new(None).await;
    let article = grabber.parse(&url, true, &Client::new()).await.unwrap();
    article.save_html(&out_path).unwrap();

    assert_eq!(
        article.title,
        Some(String::from(
            "HTTP Error 418: Fehlercode \"Ich bin eine Teekanne\" darf bleiben"
        ))
    );
    assert_eq!(article.author, Some(String::from("Hauke Gierow")));
}

#[tokio::test(flavor = "current_thread")]
async fn phoronix() {
    let out_path = PathBuf::from(r"./test_output");
    let url =
        url::Url::parse("http://www.phoronix.com/scan.php?page=article&item=amazon_ec2_bare&num=1")
            .unwrap();

    let grabber = ArticleScraper::new(None).await;
    let article = grabber.parse(&url, false, &Client::new()).await.unwrap();
    article.save_html(&out_path).unwrap();

    assert_eq!(
        article.title,
        Some(String::from(
            "Amazon EC2 Cloud Benchmarks Against Bare Metal Systems"
        ))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn youtube() {
    let out_path = PathBuf::from(r"./test_output");
    let url = url::Url::parse("https://www.youtube.com/watch?v=8KjaIumu-jI").unwrap();

    let grabber = ArticleScraper::new(None).await;
    let article = grabber.parse(&url, false, &Client::new()).await.unwrap();
    article.save_html(&out_path).unwrap();

    assert_eq!(
        article.title.as_deref(),
        Some("RIGGED! Arena Shuffler is BROKEN | 13 Land Mono Red Burn")
    );
    assert!(article
        .html
        .map(|html| html.contains("https://www.youtube.com/embed/8KjaIumu-jI?feature=oembed"))
        .unwrap_or(false));
}
