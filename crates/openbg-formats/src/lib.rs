//! Bounded readers for the original Infinity Engine resource formats.

mod are;
mod bam;
mod bif;
mod bmp;
mod cre;
mod dlg;
mod error;
mod key;
mod reader;
mod tis;
mod tlk;
mod wed;

pub use are::{Are, AreActor, AreAnimation, AreRegion};
pub use bam::{Bam, BamCycle, BamFrame};
pub use bif::{BifArchive, BifReader, OwnedResourceData, ResourceData};
pub use bmp::IndexedBitmap;
pub use cre::Cre;
pub use dlg::{Dlg, DlgState, DlgTransition};
pub use error::FormatError;
pub use key::{BifRecord, KeyIndex, ResourceRecord};
pub use tis::{compose_base_layer, compose_base_layer_with_pages, pvrz_resref, RgbaImage};
pub use tlk::Tlk;
pub use wed::{BaseOverlay, Wed};
