use crate::*;
use reqwest::Client;
use std::path::PathBuf;

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
    let url = url::Url::parse("https://www.youtube.com/watch?v=lHRkYLcmFY8").unwrap();

    let grabber = ArticleScraper::new(None).await;
    let article = grabber.parse(&url, false, &Client::new()).await.unwrap();

    assert_eq!(
        article.html,
        Some("<iframe width=\"650\" height=\"350\" frameborder=\"0\" src=\"https://www.youtube-nocookie.com/embed/lHRkYLcmFY8\" allowfullscreen></iframe>".into())
    );
}
