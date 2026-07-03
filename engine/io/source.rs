use std::io::{Read, Result};
use crate::core::resource::ResourceId;

pub trait ResourceSource: Send + Sync {
    fn open(&self, id: &ResourceId) -> Result<Box<dyn Read + Send>>;
    fn exists(&self, id: &ResourceId) -> bool;
    fn list(&self) -> Result<Vec<ResourceId>>; // for initial milestone
}