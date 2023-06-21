use std::fmt::Debug;

#[derive(Debug)]
pub struct Pair<T>
where
    T: Debug,
{
    pub value: T,
    pub parent_value: Option<T>,
}
