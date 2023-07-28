mod buffer;
pub mod buffered_markable_reader;
pub mod markable_reader;

const DEFAULT_BUFFER_SIZE: usize = 8 * 1024;
const DEFAULT_MARKER_BUFFER_SIZE: usize = 2 * 1024;
