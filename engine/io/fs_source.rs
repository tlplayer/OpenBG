use std::fs::{self, File};
use std::io::{Read, Result};
use std::path::{Path, PathBuf};

use crate::core::resource::{ResourceId, ResourceKind};
use super::source::ResourceSource;

pub struct FsSource {
    root: PathBuf,
}

impl FsSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, id: &ResourceId) -> PathBuf {
        let ext = match id.kind {
            ResourceKind::Wed => "WED",
            ResourceKind::Tis => "TIS",
            ResourceKind::Bam => "BAM",
            ResourceKind::Mos => "MOS",
            _ => "DAT",
        };
        self.root.join(format!("{}.{}", id.name, ext))
    }

    fn kind_from_ext(ext: &str) -> Option<ResourceKind> {
        match ext.to_ascii_uppercase().as_str() {
            "WED" => Some(ResourceKind::Wed),
            "TIS" => Some(ResourceKind::Tis),
            "BAM" => Some(ResourceKind::Bam),
            "MOS" => Some(ResourceKind::Mos),
            _ => None,
        }
    }
}

impl ResourceSource for FsSource {
    fn open(&self, id: &ResourceId) -> Result<Box<dyn Read + Send>> {
        let p = self.path_for(id);
        Ok(Box::new(File::open(p)?))
    }

    fn exists(&self, id: &ResourceId) -> bool {
        self.path_for(id).exists()
    }

    fn list(&self) -> Result<Vec<ResourceId>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let p = entry.path();
            if !p.is_file() { continue; }
            let (stem, ext) = match (p.file_stem(), p.extension()) {
                (Some(s), Some(e)) => (s.to_string_lossy(), e.to_string_lossy()),
                _ => continue,
            };
            if let Some(kind) = Self::kind_from_ext(&ext) {
                out.push(ResourceId::new(stem.to_string(), kind));
            }
        }
        Ok(out)
    }
}