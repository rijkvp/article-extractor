pub use self::error::ImageDownloadError;
use crate::util::Util;
use base64::Engine;
use libxml::parser::Parser;
use libxml::tree::{Document, Node, SaveOptions};
use libxml::xpath::Context;
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use reqwest::{Client, Response, Url};
use std::io::Cursor;

mod error;

pub struct ImageDownloader {
    max_size: (u32, u32),
}

impl ImageDownloader {
    pub fn new(max_size: (u32, u32)) -> Self {
        ImageDownloader { max_size }
    }

    pub async fn download_images_from_string(
        &self,
        html: &str,
        client: &Client,
    ) -> Result<String, ImageDownloadError> {
        let parser = Parser::default_html();
        let doc = parser
            .parse_string(html)
            .map_err(|_| ImageDownloadError::HtmlParse)?;

        self.download_images_from_document(&doc, client).await?;

        let options = SaveOptions {
            format: false,
            no_declaration: false,
            no_empty_tags: true,
            no_xhtml: false,
            xhtml: false,
            as_xml: false,
            as_html: true,
            non_significant_whitespace: false,
        };
        Ok(doc.to_string_with_options(options))
    }

    pub async fn download_images_from_document(
        &self,
        doc: &Document,
        client: &Client,
    ) -> Result<(), ImageDownloadError> {
        let xpath_ctx = Context::new(doc).map_err(|()| {
            log::error!("Failed to create xpath context for document");
            ImageDownloadError::HtmlParse
        })?;

        let xpath = "//img";
        let node_vec = Util::evaluate_xpath(&xpath_ctx, xpath, false)
            .map_err(|_| ImageDownloadError::HtmlParse)?;

        let mut image_urls = Vec::new();

        for node in node_vec {
            image_urls.push(Self::harvest_image_urls(node, client));
        }

        let res = futures::future::join_all(image_urls)
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();

        let mut download_futures = Vec::new();

        for (node, url, parent_url) in res {
            download_futures.push(self.download_and_replace_image(node, url, parent_url, client));
        }

        _ = futures::future::join_all(download_futures).await;

        Ok(())
    }

    async fn download_and_replace_image(
        &self,
        mut node: Node,
        image_url: Url,
        parent_url: Option<Url>,
        client: &Client,
    ) {
        _ = self
            .download_image_base64(&image_url, parent_url.as_ref(), client)
            .await
            .map(|(small, big)| {
                _ = node.set_property("src", &small);

                if let Some(big) = big {
                    _ = node.set_property("big-src", &big);
                }
            })
            .map_err(|error| log::error!("Failed to download image: {error}"));
    }

    async fn harvest_image_urls(
        node: Node,
        client: &Client,
    ) -> Result<(Node, Url, Option<Url>), ImageDownloadError> {
        let src = match node.get_property("src") {
            Some(src) => {
                if src.starts_with("data:") {
                    log::debug!("");
                    return Err(ImageDownloadError::Unknown);
                } else {
                    src
                }
            }
            None => {
                log::debug!("");
                return Err(ImageDownloadError::Unknown);
            }
        };

        let url = Url::parse(&src).map_err(ImageDownloadError::InvalidUrl)?;
        let parent_url = Self::check_image_parent(&node, &url, client).await.ok();

        Ok((node, url, parent_url))
    }

    async fn download_image_base64(
        &self,
        image_url: &Url,
        parent_url: Option<&Url>,
        client: &Client,
    ) -> Result<(String, Option<String>), ImageDownloadError> {
        let response = client.get(image_url.clone()).send().await.map_err(|err| {
            log::error!("GET {} failed - {}", image_url.as_str(), err);
            ImageDownloadError::Http
        })?;

        let content_type_small = ImageDownloader::check_image_content_type(&response)?;
        let content_type_small = content_type_small
            .to_str()
            .map_err(|_| ImageDownloadError::ContentType)?;
        let mut content_type_big: Option<String> = None;

        let mut small_image = response
            .bytes()
            .await
            .map_err(|_| ImageDownloadError::Http)?
            .as_ref()
            .to_vec();

        let mut big_image: Option<Vec<u8>> = None;

        if let Some(parent_url) = parent_url {
            let response_big = client
                .get(parent_url.clone())
                .send()
                .await
                .map_err(|_| ImageDownloadError::Http)?;
            content_type_big = Some(
                ImageDownloader::check_image_content_type(&response_big)?
                    .to_str()
                    .map_err(|_| ImageDownloadError::ContentType)?
                    .to_owned(),
            );
            big_image = Some(
                response_big
                    .bytes()
                    .await
                    .map_err(|_| ImageDownloadError::Http)?
                    .to_vec(),
            );
        }

        if content_type_small != "image/svg+xml" && content_type_small != "image/gif" {
            let (original_image, resized_image) = Self::scale_image(&small_image, self.max_size)?;
            if let Some(resized_image) = resized_image {
                small_image = resized_image;
                if big_image.is_none() {
                    big_image = Some(original_image);
                    content_type_big = Some(content_type_small.to_owned());
                }
            } else {
                small_image = original_image;
            }
        }

        let small_image_base64 = base64::engine::general_purpose::STANDARD.encode(&small_image);
        let big_image_base64 =
            big_image.map(|img| base64::engine::general_purpose::STANDARD.encode(img));
        let small_image_string =
            format!("data:{};base64,{}", content_type_small, small_image_base64);
        let big_image_string = match big_image_base64 {
            Some(big_image_base64) => {
                let content_type_big = content_type_big.ok_or_else(|| {
                    log::debug!("content_type_big should not be None when a big image exists");
                    ImageDownloadError::ParentDownload
                })?;
                Some(format!(
                    "data:{};base64,{}",
                    content_type_big, big_image_base64
                ))
            }
            None => None,
        };
        Ok((small_image_string, big_image_string))
    }

    fn check_image_content_type(response: &Response) -> Result<HeaderValue, ImageDownloadError> {
        if response.status().is_success() {
            if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
                if content_type
                    .to_str()
                    .map_err(|_| ImageDownloadError::ContentType)?
                    .contains("image")
                {
                    return Ok(content_type.clone());
                }
            }

            log::warn!("{} is not an image", response.url());
            Err(ImageDownloadError::ContentType)
        } else {
            Err(ImageDownloadError::Http)
        }
    }

    fn scale_image(
        image_buffer: &[u8],
        max_dimensions: (u32, u32),
    ) -> Result<(Vec<u8>, Option<Vec<u8>>), ImageDownloadError> {
        let mut original_image: Vec<u8> = Vec::new();
        let mut resized_image: Option<Vec<u8>> = None;

        let mut image = image::load_from_memory(image_buffer).map_err(|err| {
            log::error!("Failed to open image to resize: {}", err);
            ImageDownloadError::ImageScale
        })?;

        image
            .write_to(
                &mut Cursor::new(&mut original_image),
                image::ImageOutputFormat::Png,
            )
            .map_err(|err| {
                log::error!("Failed to save resized image to resize: {}", err);
                ImageDownloadError::ImageScale
            })?;

        let dimensions = (image.width(), image.height());
        if dimensions.0 > max_dimensions.0 || dimensions.1 > max_dimensions.1 {
            image = image.resize(
                max_dimensions.0,
                max_dimensions.1,
                image::imageops::FilterType::Lanczos3,
            );
            let mut resized_buf: Vec<u8> = Vec::new();
            image
                .write_to(
                    &mut Cursor::new(&mut resized_buf),
                    image::ImageOutputFormat::Png,
                )
                .map_err(|err| {
                    log::error!("Failed to save resized image to resize: {}", err);
                    ImageDownloadError::ImageScale
                })?;
            resized_image = Some(resized_buf);
        }

        Ok((original_image, resized_image))
    }

    async fn check_image_parent(
        node: &Node,
        child_url: &Url,
        client: &Client,
    ) -> Result<Url, ImageDownloadError> {
        let parent = match node.get_parent() {
            Some(parent) => parent,
            None => {
                log::debug!("No parent node");
                return Err(ImageDownloadError::ParentDownload);
            }
        };

        if parent.get_name().to_lowercase() != "a" {
            log::debug!("parent is not an <a> node");
            return Err(ImageDownloadError::ParentDownload);
        }

        let href = match parent.get_property("href") {
            Some(href) => href,
            None => {
                log::debug!("Parent doesn't have href prop");
                return Err(ImageDownloadError::ParentDownload);
            }
        };

        let parent_url = Url::parse(&href).map_err(|err| {
            log::debug!("Failed to parse parent image url: {}", err);
            ImageDownloadError::InvalidUrl(err)
        })?;
        let parent_response = client
            .head(parent_url.clone())
            .send()
            .await
            .map_err(|_| ImageDownloadError::Http)?;
        let _ = ImageDownloader::check_image_content_type(&parent_response)?;
        let child_response = client
            .head(child_url.clone())
            .send()
            .await
            .map_err(|_| ImageDownloadError::Http)?;
        let parent_length = Self::get_content_lenght(&parent_response)?;
        let child_length = Self::get_content_lenght(&child_response)?;

        if parent_length > child_length {
            return Ok(parent_url);
        }

        log::debug!("Image parent element not relevant");
        Err(ImageDownloadError::ParentDownload)
    }

    fn get_content_lenght(response: &Response) -> Result<u64, ImageDownloadError> {
        let status_code = response.status();

        if !status_code.is_success() {
            log::warn!("response: {status_code}");
            return Err(ImageDownloadError::Http);
        }

        response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|content_length| content_length.to_str().ok())
            .and_then(|content_length| content_length.parse::<u64>().ok())
            .ok_or(ImageDownloadError::ContentLenght)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;
    use std::fs;
    use std::io::Write;

    #[tokio::test]
    async fn fedora31() {
        let image_dowloader = ImageDownloader::new((2048, 2048));
        let html = fs::read_to_string(r"./resources/tests/planetGnome/fedora31.html")
            .expect("Failed to read HTML");
        let result = image_dowloader
            .download_images_from_string(&html, &Client::new())
            .await
            .expect("Failed to downalod images");
        let mut file = fs::File::create(r"./test_output/fedora31_images_downloaded.html")
            .expect("Failed to create output file");
        file.write_all(result.as_bytes())
            .expect("Failed to write result to file");
    }
}
