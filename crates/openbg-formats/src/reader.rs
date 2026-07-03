use crate::FormatError;

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    context: &'static str,
}

impl<'a> Reader<'a> {
    pub(crate) const fn new(bytes: &'a [u8], context: &'static str) -> Self {
        Self { bytes, context }
    }

    pub(crate) fn slice(&self, offset: usize, size: usize) -> Result<&'a [u8], FormatError> {
        let end = offset
            .checked_add(size)
            .ok_or_else(|| FormatError::bounds(self.context, offset, size, self.bytes.len()))?;
        self.bytes
            .get(offset..end)
            .ok_or_else(|| FormatError::bounds(self.context, offset, size, self.bytes.len()))
    }

    pub(crate) fn array<const N: usize>(&self, offset: usize) -> Result<[u8; N], FormatError> {
        self.slice(offset, N)?
            .try_into()
            .map_err(|_| FormatError::bounds(self.context, offset, N, self.bytes.len()))
    }

    pub(crate) fn u16(&self, offset: usize) -> Result<u16, FormatError> {
        Ok(u16::from_le_bytes(self.array(offset)?))
    }

    pub(crate) fn u32(&self, offset: usize) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.array(offset)?))
    }

    pub(crate) fn usize32(&self, offset: usize) -> Result<usize, FormatError> {
        usize::try_from(self.u32(offset)?)
            .map_err(|_| FormatError::new(self.context, "32-bit offset does not fit usize"))
    }

    pub(crate) fn records(
        &self,
        offset: usize,
        count: usize,
        stride: usize,
    ) -> Result<(), FormatError> {
        let size = count
            .checked_mul(stride)
            .ok_or_else(|| FormatError::new(self.context, "record table size overflow"))?;
        self.slice(offset, size).map(|_| ())
    }

    pub(crate) fn expect(&self, offset: usize, value: &[u8]) -> Result<(), FormatError> {
        if self.slice(offset, value.len())? == value {
            Ok(())
        } else {
            Err(FormatError::new(
                self.context,
                format!("unexpected signature/version at {offset:#x}"),
            ))
        }
    }
}
