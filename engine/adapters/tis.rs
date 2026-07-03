use std::io::Read;
use anyhow::{Result, bail};

#[derive(Debug, Clone)]
pub struct Tis {
    pub tile_count: u32,
    pub tile_size: u32, // usually 64
    // pixel data omitted; renderer will define upload path
}

pub struct TisLoader;

impl super::super::io::loader::AssetLoader<Tis> for TisLoader {
    fn load(&self, reader: &mut dyn Read) -> Result<Tis> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        if buf.len() < 16 {
            bail!("TIS too small");
        }
        Ok(Tis {
            tile_count: 0,
            tile_size: 64,
        })
    }
}