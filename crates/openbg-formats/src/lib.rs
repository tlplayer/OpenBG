//! Bounded readers for the original Infinity Engine resource formats.

mod are;
mod bam;
mod bcs;
mod bif;
mod bmp;
mod cre;
mod dlg;
mod error;
mod ids;
mod itm;
mod key;
mod reader;
mod sto;
mod tis;
mod tlk;
mod two_da;
mod wed;

pub use are::{Are, AreActor, AreAnimation, AreRegion};
pub use bam::{apply_palette, Bam, BamCycle, BamFrame};
pub use bcs::{Bcs, BcsBlock};
pub use bif::{BifArchive, BifReader, OwnedResourceData, ResourceData};
pub use bmp::{IndexedBitmap, RgbaBitmap};
pub use cre::{Cre, CreColors, CreInventory, CreItem, CreScripts};
pub use dlg::{Dlg, DlgState, DlgTransition};
pub use error::FormatError;
pub use ids::{Ids, IdsEntry};
pub use itm::{Itm, ItmAbility};
pub use key::{BifRecord, KeyIndex, ResourceRecord};
pub use sto::{Sto, StoItem};
pub use tis::{compose_base_layer, compose_base_layer_with_pages, pvrz_resref, RgbaImage};
pub use tlk::Tlk;
pub use two_da::{TwoDa, TwoDaRow};
pub use wed::{BaseOverlay, Wed};
