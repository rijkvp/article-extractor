# article scraper

The `article_scraper` crate provides a simple way to extract meaningful content from the web.
It contains two ways of locating the desired content

## 1. Rust implementation of [Full-Text RSS](https://www.fivefilters.org/full-text-rss/)

This makes use of website specific extraction rules. Which has the advantage of fast & accurate results.
The disadvantages however are: the config needs to be updated as the website changes and a new extraction rule is needed for every website.

A central repository of extraction rules and information about writing your own rules can be found here: [ftr-site-config](https://github.com/fivefilters/ftr-site-config).
Please consider contributing new rules or updates to it.

`article_scraper` embeds all the rules in the ftr-site-config repository for convenience. Custom and updated rules can be loaded from a `user_configs` path.

## 2. Mozilla Readability

In case the ftr-config based extraction fails the [mozilla Readability](https://github.com/mozilla/readability) algorithm will be used as a fall-back.
This re-implementation tries to mimic the original as closely as possible.

# Example

```
use article_scraper::ArticleScraper;
use url::Url;
use reqwest::Client;

let scraper = ArticleScraper::new(None);
let url = Url::parse("https://www.nytimes.com/interactive/2023/04/21/science/parrots-video-chat-facetime.html");
let client = Client::new();
let article = scraper.parse(&url, false, &client, None).await.unwrap();
```

# CLI

Various features of this crate can be used via [`article_scraper_cli`](./article_scraper_cli/).

```
Usage: article_scraper_cli [OPTIONS] <COMMAND>

Commands:
  all          Use the complete pipeline
  readability  Only use the Readability parser
  ftr          Only use (a subset of) the Ftr parser
  help         Print this message or the help of the given subcommand(s)

Options:
  -d, --debug          Turn debug logging on
  -o, --output <FILE>  Destination of resulting HTML file
  -h, --help           Print help
  -V, --version        Print version
```