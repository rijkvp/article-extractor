# article-extractor

This is a non-aysnc fork of [article_scraper](https://gitlab.com/news-flash/article_scraper) containing only the article extraction functionallity (does not support web crawling).

It contains two ways of extracting articles from HTML:

## 1. Rust implementation of [Full-Text RSS](https://www.fivefilters.org/full-text-rss/)

This makes use of website specific extraction rules. Which has the advantage of fast & accurate results.
The disadvantages however are: the config needs to be updated as the website changes and a new extraction rule is needed for every website.

A central repository of extraction rules and information about writing your own rules can be found here: [ftr-site-config](https://github.com/fivefilters/ftr-site-config).
Please consider contributing new rules or updates to it.

`article_scraper` embeds all the rules in the ftr-site-config repository for convenience. Custom and updated rules can be loaded from a `user_configs` path.

## 2. Mozilla Readability

In case the ftr-config based extraction fails the [mozilla Readability](https://github.com/mozilla/readability) algorithm will be used as a fall-back.
This re-implementation tries to mimic the original as closely as possible.
