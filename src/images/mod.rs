use std::path::PathBuf;
use reqwest;
use libxml::parser::Parser;
use libxml::xpath::Context;
use libxml::tree::Node;
use url;
use failure::ResultExt;
use std::error::Error;
use self::error::{ImageDownloadError, ImageDownloadErrorKind};
use base64;
use std;
use image;
use mime_guess;
use super::ScraperErrorKind;

mod error;

pub struct ImageDownloader {
    save_image_path: PathBuf,
    client: reqwest::Client,
    max_size: (u32, u32),
    scale_size: (u32, u32),
}

impl ImageDownloader {

    pub fn new(save_image_path: PathBuf, max_size: (u32, u32), scale_size: (u32, u32)) -> ImageDownloader {
        ImageDownloader {
            save_image_path: save_image_path,
            client: reqwest::Client::new(),
            max_size: max_size,
            scale_size: scale_size,
        }
    }

    pub fn download_images_from_string(&self, html: &str, article_url: &url::Url) -> Result<String, ImageDownloadError> {

        let parser = Parser::default_html();
        let doc = match parser.parse_string(html) {
            Ok(doc) => doc,
            Err(_) => {
                error!("Failed to parse HTML string");
                return Err(ImageDownloadErrorKind::HtmlParse)?
            }
        };

        let xpath_ctx = match Context::new(&doc) {
            Ok(context) => context,
            Err(_) => {
                error!("Failed to create xpath context for document");
                return Err(ImageDownloadErrorKind::HtmlParse)?
            }
        };

        self.download_images_from_context(&xpath_ctx, article_url)?;

        Ok(doc.to_string(/*format:*/ false))
    }

    pub fn download_images_from_context(&self, context: &Context, article_url: &url::Url) -> Result<(), ImageDownloadError> {
        let xpath = "//img";
        evaluate_xpath!(context, xpath, node_vec);
        for mut node in node_vec {
            if let Some(url) = node.get_property("src") {
                let url = url::Url::parse(&url).context(ImageDownloadErrorKind::InvalidUrl)?;
                let parent_url_result = match self.check_image_parent(&node, &url) {
                    Ok(url) => Some(url),
                    Err(_) => None,
                };

                if let Some(parent_url) = parent_url_result.clone() {
                    if let Ok(path) = self.save_image(&parent_url, article_url) {
                        if let Some(path) = path.to_str() {
                            if let Err(_) = node.set_property("parent_img", path) {
                                return Err(ImageDownloadErrorKind::HtmlParse)?;
                            }
                        }
                    }
                }

                let mut img_path = self.save_image(&url, article_url)?;

                if let Some((width, height)) = ImageDownloader::get_image_dimensions(&node) {
                    if width > self.max_size.0 || height > self.max_size.1 {
                        if let Ok(small_img_path) = ImageDownloader::scale_image(&img_path, self.scale_size.0, self.scale_size.1) {
                            if parent_url_result.is_none() {
                                if let Some(img_path) = img_path.to_str() {
                                    if let Err(_) = node.set_property("big_img", img_path) {
                                        return Err(ImageDownloadErrorKind::HtmlParse)?;
                                    }
                                }

                                img_path = small_img_path;
                            }
                        }
                    }
                }

                if let Some(img_path) = img_path.to_str() {
                    if let Err(_) = node.set_property("src", img_path) {
                        return Err(ImageDownloadErrorKind::HtmlParse)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn save_image(&self, image_url: &url::Url, article_url: &url::Url) -> Result<PathBuf, ImageDownloadError> {

        let mut response = match self.client.get(image_url.clone()).send() {
            Ok(response) => response,
            Err(error) => {
                error!("GET {} failed - {}", image_url.as_str(), error.description());
                Err(error).context(ImageDownloadErrorKind::Http)?
            }
        };
        let content_type = ImageDownloader::check_image_content_type(&response)?;
        
        if let Some(host) = article_url.host_str() {
            let folder_name = base64::encode(article_url.as_str()).replace("/", "_");
            let path = self.save_image_path.join(host);
            let path = path.join(folder_name);

            if let Ok(()) = std::fs::create_dir_all(&path) {
                let file_name = ImageDownloader::extract_image_name(image_url, content_type)?;
                let path = path.join(file_name);
                let mut image_buffer = match std::fs::File::create(&path) {
                    Ok(buffer) => buffer,
                    Err(error) => {
                        error!("Failed to create file {}", path.display());
                        Err(error).context(ImageDownloadErrorKind::IO)?
                    }
                };

                response.copy_to(&mut image_buffer).context(ImageDownloadErrorKind::IO)?;
                let path = std::fs::canonicalize(&path).context(ImageDownloadErrorKind::IO)?;
                return Ok(path)
            }
        }

        Err(ImageDownloadErrorKind::InvalidUrl)?
    }

    fn check_image_content_type(response: &reqwest::Response) -> Result<reqwest::header::HeaderValue, ImageDownloadError> {
    
        if response.status().is_success() {
            if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
                if content_type.to_str().context(ImageDownloadErrorKind::ContentType)?.contains("image") {
                    return Ok(content_type.clone())
                }
            }

            error!("{} is not an image", response.url());
            return Err(ImageDownloadErrorKind::ContentType)?
        }

        Err(ImageDownloadErrorKind::Http)?
    }

    fn get_content_lenght(response: &reqwest::Response) -> Result<u64, ImageDownloadError> {

        if response.status().is_success() {
            if let Some(content_length) = response.headers().get(reqwest::header::CONTENT_LENGTH) {
                if let Ok(content_length) = content_length.to_str() {
                    if let Ok(content_length) = content_length.parse::<u64>() {
                        return Ok(content_length)
                    }
                }
            }
        }

        Err(ImageDownloadErrorKind::ContentLenght)?
    }

    fn get_image_dimensions(node: &Node) -> Option<(u32, u32)> {

        if let Some(width) = node.get_property("width") {
            if let Some(height) = node.get_property("height") {
                if let Ok(width) = width.parse::<u32>() {
                    if let Ok(height) = height.parse::<u32>() {
                        if width > 1 && height > 1 {
                            return Some((width, height))
                        }
                    }
                }
            }
        }

        debug!("Image dimensions not available");
        None
    }

    fn extract_image_name(url: &url::Url, content_type: reqwest::header::HeaderValue) -> Result<String, ImageDownloadError> {

        if let Some(file_name) = url.path_segments().and_then(|segments| segments.last()) {
            let mut image_name = file_name.to_owned();
            if let Some(query) = url.query() {
                image_name.push_str("_");
                image_name.push_str(query);
            }

            let header = content_type.to_str().context(ImageDownloadErrorKind::ContentType)?;
            let primary_type = match header.find("/") {
                Some(end) => header[..end-1].to_string(),
                None => "unknown".to_string(),
            };
            let mut sub_type = match header.find("/") {
                None => "unknown".to_string(),
                Some(start) => {
                    match header.find("+") {
                        None => "unknown".to_string(),
                        Some(end) => header[start..end-1].to_string(),
                    }
                },
            };
            if let Some(start) = header.find("+") {
                sub_type.push_str("+");
                sub_type.push_str(&header[start..].to_string());
            };

            if let Some(extensions) = mime_guess::get_extensions(&primary_type, &sub_type) {
                let mut extension_present = false;
                for extension in extensions {
                    if image_name.ends_with(extension) {
                        extension_present = true;
                        break;
                    }
                }

                if !extension_present {
                    image_name.push_str(".");
                    image_name.push_str(extensions[0]);
                }
            }
            
            return Ok(image_name)
        }

        error!("Could not generate image name for {}", url.as_str());
        Err(ImageDownloadErrorKind::ImageName)?
    }

    fn check_image_parent(&self, node: &Node, child_url: &url::Url) -> Result<url::Url, ImageDownloadError> {

        if let Some(parent) = node.get_parent() {
            if parent.get_name() == "a" {
                if let Some(url) = parent.get_property("href") {
                    let parent_url = url::Url::parse(&url).context(ImageDownloadErrorKind::ParentDownload)?;
                    let parent_response = self.client.head(parent_url.clone()).send().context(ImageDownloadErrorKind::ParentDownload)?;
                    let _ = ImageDownloader::check_image_content_type(&parent_response).context(ImageDownloadErrorKind::ParentDownload)?;
                    let child_response = self.client.get(child_url.clone()).send().context(ImageDownloadErrorKind::ParentDownload)?;
                    let parent_length = ImageDownloader::get_content_lenght(&parent_response).context(ImageDownloadErrorKind::ParentDownload)?;
                    let child_length = ImageDownloader::get_content_lenght(&child_response).context(ImageDownloadErrorKind::ParentDownload)?;

                    if parent_length > child_length {
                        return Ok(parent_url)
                    }

                    return Ok(child_url.clone())
                }
            }
        }

        debug!("Image parent element not relevant");
        Err(ImageDownloadErrorKind::ParentDownload)?
    }

    fn scale_image(image_path: &PathBuf, max_width: u32, max_height: u32) -> Result<PathBuf, ImageDownloadError> {

        let image = match image::open(image_path) {
            Ok(image) => image,
            Err(error) => {
                error!("Failed to open image to resize: {:?}", image_path);
                return Err(error).context(ImageDownloadErrorKind::ImageScale)?
            }
        };
        let image = image.resize(max_width, max_height, image::FilterType::Lanczos3);

        if let Some(file_name) = image_path.file_name() {

            let mut file_name = file_name.to_os_string();
            file_name.push("_resized");
            let mut resized_path = image_path.clone();
            resized_path.set_file_name(file_name);
            if let Err(error) = image.save(&resized_path) {
                error!("Failed to write resized image to disk.");
                return Err(error).context(ImageDownloadErrorKind::ImageScale)?
            }
                
            return Ok(resized_path)
        }

        Err(ImageDownloadErrorKind::ImageScale)?
    }
}