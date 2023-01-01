pub struct State {
    pub strip_unlikely: bool,
    pub weigh_classes: bool,
    pub clean_conditionally: bool,
    pub should_remove_title_header: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            strip_unlikely: true,
            weigh_classes: true,
            clean_conditionally: true,
            should_remove_title_header: true,
        }
    }
}