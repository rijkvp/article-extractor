use url::Url;

pub enum Page {
    Single(Url),
    Multi(Option<Url>),
}
