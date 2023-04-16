pub use self::error::ImageDownloadError;
use self::request::ImageRequest;
use crate::util::Util;
use base64::Engine;
use image::ImageOutputFormat;
use libxml::parser::Parser;
use libxml::tree::{Document, Node, SaveOptions};
use libxml::xpath::Context;
pub use progress::Progress;
use reqwest::{Client, Url};
use std::io::Cursor;
use tokio::sync::mpsc::{self, Sender};

mod error;
mod progress;
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
        progress: Option<Sender<Progress>>,
    ) -> Result<String, ImageDownloadError> {
        let parser = Parser::default_html();
        let doc = parser
            .parse_string(html)
            .map_err(|_| ImageDownloadError::HtmlParse)?;

        self.download_images_from_document(&doc, client, progress)
            .await?;

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
        progress: Option<Sender<Progress>>,
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

        let total_size = res
            .iter()
            .map(|(req, parent_req)| {
                req.content_length() + parent_req.as_ref().map(|r| r.content_length()).unwrap_or(0)
            })
            .sum::<usize>();

        let (tx, mut rx) = mpsc::channel::<usize>(2);

        let mut download_futures = Vec::new();

        for (request, parent_request) in res {
            download_futures.push(self.download_and_replace_image(
                request,
                parent_request,
                tx.clone(),
            ));
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

        _ = futures::future::join_all(download_futures).await;

        Ok(())
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
        let parent_url = Self::check_image_parent(&node).await.ok();

        let request = ImageRequest::new(node.clone(), &url, client).await?;
        let parent_request = match parent_url {
            Some(parent_url) => Some(ImageRequest::new(node, &parent_url, client).await?),
            None => None,
        };

        Ok((request, parent_request))
    }

    async fn download_and_replace_image(
        &self,
        mut request: ImageRequest,
        mut parent_request: Option<ImageRequest>,
        tx: Sender<usize>,
    ) -> Result<(), ImageDownloadError> {
        let mut image = request.download(&tx).await?;
        let mut parent_image: Option<Vec<u8>> = None;

        if let Some(parent_request) = parent_request.as_mut() {
            if parent_request.content_length() > request.content_length() {
                parent_image = parent_request.download(&tx).await.ok();
            }
        }

        if request.content_type() != "image/svg+xml" && request.content_type() != "image/gif" {
            if let Some(resized_image) = Self::scale_image(&image, self.max_size) {
                if parent_image.is_none() {
                    parent_image = Some(image);
                }
                image = resized_image;
            }
        }

        let image_base64 = base64::engine::general_purpose::STANDARD.encode(&image);
        let image_string = format!("data:{};base64,{}", request.content_type(), image_base64);
        request.write_image_to_property("src", &image_string);

        if let Some(parent_image) = parent_image {
            let parent_image_base64 =
                base64::engine::general_purpose::STANDARD.encode(parent_image);

            let content_type = parent_request
                .map(|pr| pr.content_type().to_string())
                .unwrap_or(request.content_type().to_string());
            let parent_image_string =
                format!("data:{};base64,{}", content_type, parent_image_base64);

            request.write_image_to_property("big-src", &parent_image_string);
        }

        Ok(())
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

    async fn check_image_parent(node: &Node) -> Result<Url, ImageDownloadError> {
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

        Ok(parent_url)
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
            .download_images_from_string(&html, &Client::new(), None)
            .await
            .expect("Failed to downalod images");
        let mut file = fs::File::create(r"./test_output/fedora31_images_downloaded.html")
            .expect("Failed to create output file");
        file.write_all(result.as_bytes())
            .expect("Failed to write result to file");
    }
}
