use crate::compression_algorithm::CompressionAlgorithm;
use crate::packet_error::PacketError;
#[cfg(feature = "compression_lz4")]
use lz4_flex::block::{self, CompressTable};

/// Transport-wide compression workspace retained across compression attempts.
///
/// LZ4's hash table is shared across compression operations. [`Self::compress`] writes into the
/// owned output buffer, while [`Self::compress_into`] allows callers to retain their own buffer.
#[derive(Default)]
pub(crate) struct CompressionScratch {
    output: Vec<u8>,
    /// The packet bodies compressed, which the builder's tests bound.
    #[cfg(test)]
    pub(crate) compressions: usize,
    #[cfg(feature = "compression_lz4")]
    lz4_table: Option<CompressTable>,
}

impl core::fmt::Debug for CompressionScratch {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut state = formatter.debug_struct("CompressionScratch");
        state.field("output_capacity", &self.output.capacity());
        #[cfg(test)]
        state.field("compressions", &self.compressions);
        #[cfg(feature = "compression_lz4")]
        state.field("lz4_table_initialized", &self.lz4_table.is_some());
        state.finish()
    }
}

impl CompressionScratch {
    #[cfg_attr(
        not(feature = "compression_lz4"),
        expect(
            unused_variables,
            clippy::missing_const_for_fn,
            clippy::needless_pass_by_ref_mut,
            clippy::ptr_arg,
            reason = "no compression backend is compiled in"
        )
    )]
    pub(crate) fn compress_into<'output>(
        &mut self,
        payload: &[u8],
        algorithm: CompressionAlgorithm,
        output: &'output mut Vec<u8>,
    ) -> Result<&'output [u8], PacketError> {
        #[cfg(test)]
        {
            self.compressions += 1;
        }
        match algorithm {
            #[cfg(feature = "compression_lz4")]
            CompressionAlgorithm::Lz4 => Self::compress_lz4(
                payload,
                output,
                self.lz4_table.get_or_insert_with(CompressTable::default),
            ),
        }
    }

    #[cfg_attr(
        not(feature = "compression_lz4"),
        expect(
            unused_variables,
            clippy::missing_const_for_fn,
            clippy::needless_pass_by_ref_mut,
            reason = "no compression backend is compiled in"
        )
    )]
    pub(crate) fn compress(
        &mut self,
        payload: &[u8],
        algorithm: CompressionAlgorithm,
    ) -> Result<&[u8], PacketError> {
        match algorithm {
            #[cfg(feature = "compression_lz4")]
            CompressionAlgorithm::Lz4 => Self::compress_lz4(
                payload,
                &mut self.output,
                self.lz4_table.get_or_insert_with(CompressTable::default),
            ),
        }
    }

    /// The first `len` bytes of the last [`Self::compress`] output.
    pub(crate) fn output(&self, len: usize) -> &[u8] {
        &self.output[..len]
    }

    #[cfg(feature = "compression_lz4")]
    pub(crate) fn compress_lz4<'output>(
        payload: &[u8],
        output: &'output mut Vec<u8>,
        table: &mut CompressTable,
    ) -> Result<&'output [u8], PacketError> {
        const SIZE_PREFIX_BYTES: usize = size_of::<u32>();

        let uncompressed_len =
            u32::try_from(payload.len()).map_err(|_| PacketError::CompressionFailed)?;
        let output_len = SIZE_PREFIX_BYTES + block::get_maximum_output_size(payload.len());
        if output.len() < output_len {
            output.resize(output_len, 0);
        }
        output[..SIZE_PREFIX_BYTES].copy_from_slice(&uncompressed_len.to_le_bytes());

        let compressed_len = block::compress_into_with_table(
            payload,
            &mut output[SIZE_PREFIX_BYTES..output_len],
            table,
        )
        .map_err(|_| PacketError::CompressionFailed)?;

        Ok(&output[..SIZE_PREFIX_BYTES + compressed_len])
    }
}
