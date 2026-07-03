use std::io::Read;

pub trait AssetLoader<T>: Send + Sync {
    fn load(&self, reader: &mut dyn Read) -> anyhow::Result<T>;
}