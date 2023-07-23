pub use self::error::ImageDownloadError;
use self::image_data::ImageDataBase64;
use self::pair::Pair;
use self::request::ImageRequest;
use crate::util::Util;
use base64::Engine;
use futures::StreamExt;
use image::ImageOutputFormat;
use libxml::parser::Parser;
use libxml::tree::{Node, SaveOptions};
use libxml::xpath::Context;
pub use progress::Progress;
use reqwest::Client;
use std::io::Cursor;
use tokio::sync::mpsc::{self, Sender};

mod error;
mod image_data;
mod pair;
mod progress;
mod request;

pub struct ImageDownloader {
    max_size: (u32, u32),
}

impl ImageDownloader {
    pub fn new(max_size: (u32, u32)) -> Self {
        ImageDownloader { max_size }
    }

    pub async fn single_from_url(
        url: &str,
        client: &Client,
        progress: Option<Sender<Progress>>,
    ) -> Result<Vec<u8>, ImageDownloadError> {
        let response = client.get(url).send().await?;

        let content_type = Util::get_content_type(&response)?;
        let content_length = Util::get_content_length(&response).unwrap_or(0);

        if !content_type.contains("image") {
            return Err(ImageDownloadError::ContentType);
        }

        let mut stream = response.bytes_stream();
        let mut downloaded_bytes = 0;

        let mut result = Vec::with_capacity(content_length);
        while let Some(item) = stream.next().await {
            let chunk = item?;
            downloaded_bytes += chunk.len();

            if let Some(sender) = progress.as_ref() {
                _ = sender
                    .send(Progress {
                        total_size: content_length,
                        downloaded: downloaded_bytes,
                    })
                    .await;
            }

            for byte in chunk {
                result.push(byte);
            }
        }

        Ok(result)
    }

    pub async fn download_images_from_string(
        &self,
        html: &str,
        client: &Client,
        progress: Option<Sender<Progress>>,
    ) -> Result<String, ImageDownloadError> {
        let image_urls = Self::harvest_image_urls_from_html(html)?;

        let mut image_requests = Vec::new();
        for image_url in image_urls {
            let client = client.clone();
            let future = async move {
                let request = ImageRequest::new(image_url.value, &client).await;
                let parent_request = if let Some(parent_url) = image_url.parent_value {
                    ImageRequest::new(parent_url, &client).await.ok()
                } else {
                    None
                };

                if let Ok(request) = request {
                    Some(Pair {
                        value: request,
                        parent_value: parent_request,
                    })
                } else {
                    None
                }
            };
            image_requests.push(future);
        }

        let res = futures::future::join_all(image_requests)
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let total_size = res
            .iter()
            .map(|req_pair| {
                req_pair.value.content_length()
                    + req_pair
                        .parent_value
                        .as_ref()
                        .map(|p| p.content_length())
                        .unwrap_or(0)
            })
            .sum::<usize>();

        let (tx, mut rx) = mpsc::channel::<usize>(2);

        let mut download_futures = Vec::new();

        for request in res {
            download_futures.push(self.download_image(request, tx.clone()));
        }

        tokio::spawn(async move {
            let mut received = 0_usize;

            while let Some(i) = rx.recv().await {
                received += i;

                if let Some(progress) = progress.as_ref() {
                    _ = progress
                        .send(Progress {
                            total_size,
                            downloaded: received,
                        })
                        .await;
                }
            }
        });

        let downloaded_images = futures::future::join_all(download_futures)
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        Self::replace_downloaded_images(html, downloaded_images)
    }

    fn replace_downloaded_images(
        html: &str,
        downloaded_images: Vec<Pair<ImageDataBase64>>,
    ) -> Result<String, ImageDownloadError> {
        let parser = Parser::default_html();
        let doc = parser
            .parse_string(html)
            .map_err(|_| ImageDownloadError::HtmlParse)?;

        let xpath_ctx = Context::new(&doc).map_err(|()| {
            log::error!("Failed to create xpath context for document");
            ImageDownloadError::HtmlParse
        })?;

        for downloaded_image_pair in downloaded_images {
            let xpath = format!("//img[@src='{}']", &downloaded_image_pair.value.url);
            let node = Util::evaluate_xpath(&xpath_ctx, &xpath, false)
                .expect("doesn't throw")
                .into_iter()
                .next();

            if let Some(mut node) = node {
                if node
                    .set_property("src", &downloaded_image_pair.value.data)
                    .is_err()
                {
                    continue;
                }

                if let Some(parent_data) = downloaded_image_pair.parent_value {
                    _ = node.set_property("big-src", &parent_data.data)
                }
            }
        }

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

    fn harvest_image_urls_from_html(html: &str) -> Result<Vec<Pair<String>>, ImageDownloadError> {
        let parser = Parser::default_html();
        let doc = parser
            .parse_string(html)
            .map_err(|_| ImageDownloadError::HtmlParse)?;

        let xpath_ctx = Context::new(&doc).map_err(|()| {
            log::error!("Failed to create xpath context for document");
            ImageDownloadError::HtmlParse
        })?;

        let xpath = "//img";
        let node_vec = Util::evaluate_xpath(&xpath_ctx, xpath, false)
            .map_err(|_| ImageDownloadError::HtmlParse)?;

        let mut image_urls = Vec::new();

        for node in node_vec {
            image_urls.push(Self::harvest_image_urls_from_node(node)?);
        }

        Ok(image_urls)
    }

    fn harvest_image_urls_from_node(node: Node) -> Result<Pair<String>, ImageDownloadError> {
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

        let parent_url = Self::check_image_parent(&node).ok();

        let image_url = Pair {
            value: src,
            parent_value: parent_url,
        };

        Ok(image_url)
    }

    async fn download_image(
        &self,
        request: Pair<ImageRequest>,
        tx: Sender<usize>,
    ) -> Result<Pair<ImageDataBase64>, ImageDownloadError> {
        let content_type = request.value.content_type().to_owned();
        let scale_image = content_type != "image/svg+xml" && content_type != "image/gif";
        let mut image = request.value.download(&tx).await?;
        let mut parent_image = None;
        if let Some(parent_request) = request.parent_value {
            parent_image = parent_request.download(&tx).await.ok();
        }

        if scale_image {
            if let Some(resized_image) = Self::scale_image(&image.data, self.max_size) {
                if parent_image.is_none() {
                    parent_image = Some(image.clone());
                }
                image.data = resized_image;
            }
        }

        let image_base64 = base64::engine::general_purpose::STANDARD.encode(&image.data);
        let image_string = format!("data:{};base64,{}", content_type, image_base64);
        let image_data_base64 = ImageDataBase64 {
            url: image.url,
            data: image_string,
        };

        let parent_image_data_base64 = if let Some(parent_image) = parent_image {
            let parent_image_base64 =
                base64::engine::general_purpose::STANDARD.encode(&parent_image.data);
            let parent_image_string = format!(
                "data:{};base64,{}",
                parent_image.content_type, parent_image_base64
            );

            Some(ImageDataBase64 {
                url: parent_image.url,
                data: parent_image_string,
            })
        } else {
            None
        };

        Ok(Pair {
            value: image_data_base64,
            parent_value: parent_image_data_base64,
        })
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

    fn check_image_parent(node: &Node) -> Result<String, ImageDownloadError> {
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

        Ok(href)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;
    use std::fs;

    #[tokio::test]
    #[ignore = "downloads content from the web"]
    async fn fedora31() {
        let image_dowloader = ImageDownloader::new((2048, 2048));

        let html = fs::read_to_string(r"./resources/tests/images/planet_gnome/source.html")
            .expect("Failed to read HTML");
        let result = image_dowloader
            .download_images_from_string(&html, &Client::new(), None)
            .await
            .expect("Failed to downalod images");
        let expected = fs::read_to_string(r"./resources/tests/images/planet_gnome/expected.html")
            .expect("Failed to create output file");
        assert_eq!(expected, result);
    }
}
