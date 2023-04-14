pub use self::error::ImageDownloadError;
use self::request::ImageRequest;
use crate::util::Util;
use base64::Engine;
use image::ImageOutputFormat;
use libxml::parser::Parser;
use libxml::tree::{Document, Node, SaveOptions};
use libxml::xpath::Context;
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use reqwest::{Client, Response, Url};
use std::io::Cursor;

mod error;
mod request;

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

        for (request, parent_request) in res {
            if let Some(parent_request) = parent_request {
                if parent_request.content_lenght > request.content_lenght {
                    download_futures
                        .push(self.download_and_replace_image(parent_request, "big-src"));
                }
            }

            download_futures.push(self.download_and_replace_image(request, "src"));
        }

        _ = futures::future::join_all(download_futures).await;

        Ok(())
    }

    async fn download_and_replace_image(&self, request: ImageRequest, prop_name: &str) {
        let ImageRequest {
            mut node,
            http_response,
            content_lenght,
            content_type,
        } = request;

        _ = self
            .download_image_base64(http_response, content_lenght, content_type)
            .await
            .map(|image| {
                _ = node.set_property(prop_name, &image);
            })
            .map_err(|error| log::error!("Failed to download image: {error}"));
    }

    async fn harvest_image_urls(
        node: Node,
        client: &Client,
    ) -> Result<(ImageRequest, Option<ImageRequest>), ImageDownloadError> {
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
        let parent_request = Self::check_image_parent(&node, client).await.ok();

        println!("url: {url}");

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|_| ImageDownloadError::Http)?;
        let content_type = ImageDownloader::get_content_type(&response);
        let content_lenght = Self::get_content_lenght(&response).unwrap_or(0);

        let request = ImageRequest {
            node,
            http_response: response,
            content_lenght,
            content_type,
        };

        Ok((request, parent_request))
    }

    async fn download_image_base64(
        &self,
        http_response: Response,
        content_length: u64,
        content_type: Option<HeaderValue>,
    ) -> Result<String, ImageDownloadError> {
        if content_length == 0 {
            return Err(ImageDownloadError::ContentLenght);
        }

        let content_type = content_type
            .as_ref()
            .and_then(|content_type| content_type.to_str().ok())
            .ok_or(ImageDownloadError::ContentType)?;

        if !content_type.contains("image") {
            return Err(ImageDownloadError::ContentType);
        }

        let mut image = http_response
            .bytes()
            .await
            .map_err(|_| ImageDownloadError::Http)?
            .as_ref()
            .to_vec();

        if content_type != "image/svg+xml" && content_type != "image/gif" {
            if let Some(resized_image) = Self::scale_image(&image, self.max_size) {
                image = resized_image;
            }
        }

        let image_base64 = base64::engine::general_purpose::STANDARD.encode(&image);
        let image_string = format!("data:{};base64,{}", content_type, image_base64);
        Ok(image_string)
    }

    fn get_content_type(response: &Response) -> Option<HeaderValue> {
        if response.status().is_success() {
            response.headers().get(CONTENT_TYPE).cloned()
        } else {
            None
        }
    }

    fn scale_image(image_buffer: &[u8], max_dimensions: (u32, u32)) -> Option<Vec<u8>> {
        let mut image = match image::load_from_memory(image_buffer) {
            Err(error) => {
                log::error!("Failed to open image to resize: {}", error);
                return None;
            }
            Ok(image) => image,
        };

        let dimensions = (image.width(), image.height());
        if dimensions.0 > max_dimensions.0 || dimensions.1 > max_dimensions.1 {
            image = image.resize(
                max_dimensions.0,
                max_dimensions.1,
                image::imageops::FilterType::Lanczos3,
            );
            let mut resized_buf: Vec<u8> = Vec::new();
            if let Err(error) =
                image.write_to(&mut Cursor::new(&mut resized_buf), ImageOutputFormat::Png)
            {
                log::error!("Failed to save resized image to resize: {}", error);
                return None;
            }

            Some(resized_buf)
        } else {
            None
        }
    }

    async fn check_image_parent(
        node: &Node,
        client: &Client,
    ) -> Result<ImageRequest, ImageDownloadError> {
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

        println!("parent url: {parent_url}");

        let response = client
            .get(parent_url.clone())
            .send()
            .await
            .map_err(|_| ImageDownloadError::Http)?;
        let content_type = ImageDownloader::get_content_type(&response);
        let content_lenght = Self::get_content_lenght(&response).unwrap_or(0);

        Ok(ImageRequest {
            node: parent,
            http_response: response,
            content_lenght,
            content_type,
        })
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
