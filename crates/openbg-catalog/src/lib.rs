//! Read-only resource resolution for an installed Infinity Engine game.

use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use openbg_domain::{ResourceId, ResourceKind};
use openbg_formats::{BifReader, FormatError, KeyIndex, OwnedResourceData};

/// Minimal read-only resource interface consumed by content loaders.
///
/// Keeping this interface below typed decoding allows tests, imported data
/// folders, mod stacks, and original installations to share the same loaders.
pub trait ResourceCatalog {
    fn contains(&self, id: &ResourceId) -> bool;
    fn read(&self, id: &ResourceId) -> Result<OwnedResourceData, CatalogError>;

    fn read_file(&self, id: &ResourceId) -> Result<Vec<u8>, CatalogError> {
        match self.read(id)? {
            OwnedResourceData::File(bytes) => Ok(bytes),
            OwnedResourceData::Tileset { .. } => Err(CatalogError::ExpectedFile(id.clone())),
        }
    }
}

/// A validated, read-only view of one game installation.
pub struct GameInstall {
    root: PathBuf,
    index: KeyIndex,
    dialogue_table: Option<PathBuf>,
}

impl GameInstall {
    /// Opens the installation's `chitin.key` resource index.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError`] when the KEY cannot be read or parsed.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, CatalogError> {
        let root = root.into();
        let key_path = root.join("chitin.key");
        let bytes = read(&key_path)?;
        let index = KeyIndex::parse(&bytes).map_err(|source| CatalogError::Format {
            resource: None,
            source,
        })?;
        let dialogue_table = find_dialogue_table(&root);
        Ok(Self {
            root,
            index,
            dialogue_table,
        })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, id: &ResourceId) -> Result<OwnedResourceData, CatalogError> {
        if id.kind == ResourceKind::Tlk && id.resref.as_str() == "DIALOG" {
            let path = self
                .dialogue_table
                .as_ref()
                .ok_or_else(|| CatalogError::NotFound(id.clone()))?;
            return read(path).map(OwnedResourceData::File);
        }
        let record = self
            .index
            .find(id)
            .ok_or_else(|| CatalogError::NotFound(id.clone()))?;
        let bif =
            self.index
                .bifs
                .get(record.bif_index())
                .ok_or_else(|| CatalogError::MissingBif {
                    resource: id.clone(),
                    index: record.bif_index(),
                })?;
        let path = self.root.join(&bif.path);
        let file = File::open(&path).map_err(|source| CatalogError::Io {
            path: path.clone(),
            source,
        })?;
        let mut archive = BifReader::new(file).map_err(|source| CatalogError::Format {
            resource: Some(id.clone()),
            source,
        })?;
        archive
            .resource(record.locator, id.kind == ResourceKind::Tis)
            .map_err(|source| CatalogError::Format {
                resource: Some(id.clone()),
                source,
            })
    }
}

impl ResourceCatalog for GameInstall {
    fn contains(&self, id: &ResourceId) -> bool {
        if id.kind == ResourceKind::Tlk && id.resref.as_str() == "DIALOG" {
            return self.dialogue_table.is_some();
        }
        self.index.find(id).is_some()
    }

    fn read(&self, id: &ResourceId) -> Result<OwnedResourceData, CatalogError> {
        self.resolve(id)
    }
}

fn find_dialogue_table(root: &Path) -> Option<PathBuf> {
    let classic = root.join("dialog.tlk");
    if classic.is_file() {
        return Some(classic);
    }
    if let Some(locale) = std::env::var_os("OPENBG_LANG") {
        let configured = root.join("lang").join(locale).join("dialog.tlk");
        if configured.is_file() {
            return Some(configured);
        }
    }
    let english = root.join("lang/en_US/dialog.tlk");
    if english.is_file() {
        return Some(english);
    }
    let mut candidates = fs::read_dir(root.join("lang"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("dialog.tlk"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().next()
}

fn read(path: &Path) -> Result<Vec<u8>, CatalogError> {
    fs::read(path).map_err(|source| CatalogError::Io {
        path: path.to_owned(),
        source,
    })
}

#[derive(Debug)]
pub enum CatalogError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Format {
        resource: Option<ResourceId>,
        source: FormatError,
    },
    NotFound(ResourceId),
    MissingBif {
        resource: ResourceId,
        index: usize,
    },
    ExpectedFile(ResourceId),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Format {
                resource: Some(resource),
                source,
            } => write!(formatter, "{resource:?}: {source}"),
            Self::Format {
                resource: None,
                source,
            } => source.fmt(formatter),
            Self::NotFound(resource) => write!(formatter, "resource not found: {resource:?}"),
            Self::MissingBif { resource, index } => {
                write!(
                    formatter,
                    "{resource:?} references missing BIF index {index}"
                )
            }
            Self::ExpectedFile(resource) => write!(formatter, "{resource:?} is a tileset"),
        }
    }
}

impl Error for CatalogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Format { source, .. } => Some(source),
            Self::NotFound(_) | Self::MissingBif { .. } | Self::ExpectedFile(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use openbg_domain::{ResRef, ResourceId, ResourceKind};

    use super::{CatalogError, GameInstall};

    #[test]
    fn missing_install_reports_the_key_path() {
        let root = PathBuf::from("this-install-does-not-exist");
        let error = GameInstall::open(&root).err().expect("open should fail");
        match error {
            CatalogError::Io { path, .. } => assert_eq!(path, root.join("chitin.key")),
            other => panic!("expected I/O error, got {other}"),
        }
    }

    #[test]
    fn resource_id_remains_the_catalog_boundary() {
        let id = ResourceId::new(
            ResRef::new("AR2600").expect("valid resref"),
            ResourceKind::Are,
        );
        assert_eq!(id.resref.as_str(), "AR2600");
    }
}
