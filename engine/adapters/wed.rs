use std::io::{Read};
use anyhow::{Result, bail};

#[derive(Debug, Clone)]
pub struct Wed {
    pub width: u32,
    pub height: u32,
    pub tileset_resref: String, // points to TIS
    // real format has overlays, doors, walls, etc.
}

pub struct WedLoader;

impl super::super::io::loader::AssetLoader<Wed> for WedLoader {
    fn load(&self, reader: &mut dyn Read) -> Result<Wed> {
        // Placeholder parser. Replace with real WED parsing.
        // Validate header, read fields, etc.
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        if buf.len() < 16 {
            bail!("WED too small");
        }
        // fake values to unblock pipeline
        Ok(Wed {
            width: 64,
            height: 64,
            tileset_resref: "DEFAULT".to_string(),
        })
    }
}