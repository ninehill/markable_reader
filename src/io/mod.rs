mod buffer;
mod buffered_markable_reader;
mod markable_reader;

pub use buffered_markable_reader::BufferedMarkableReader;
pub use markable_reader::MarkableReader;

const DEFAULT_BUFFER_SIZE: usize = 8 * 1024;
const DEFAULT_MARKER_BUFFER_SIZE: usize = 2 * 1024;

pub trait MarkerStream {
    // Marks the location of the inner stream. From tis point forward
    /// reads will be cached. If the stream was marked prior to this call
    /// the current buffer will be discarded.
    ///
    /// Returns the number of bytes that were discarded as a result of this operation
    fn mark(&mut self) -> usize;

    /// Resets the stream previously marked position, if it is set.
    /// If the reader was not previously marked, this has no affect.
    fn reset(&mut self);

    /// Clears the current buffer dropping any values that have been cached.
    fn clear_buffer(&mut self);
}
