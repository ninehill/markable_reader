use std::io::Write;

use super::{buffer::Buffer, DEFAULT_BUFFER_SIZE, DEFAULT_MARKER_BUFFER_SIZE};

/// Reads bytes from the inner source with the additional ability
/// to `mark` a stream at a point that can be returned to later
/// using the a call to `reset()`. This reader also makes large, infrequent,
/// reads to the underlying reader to increase effeciency read, which
/// ideal when system calls may be involved (e.g., File reads), but offer
/// little benefit with in-memory readers (e.g., vecs).
///
/// Whlie the stream is marked all susequent reads are returned as usual,
/// but are also buffered, which is what allows for returning to a previous
/// part of the stream.
pub struct BufferedMarkableReader<R> {
    inner: R,
    inner_complete: bool,
    is_marked: bool,
    mark_buffer: Buffer,
    read_buffer: Buffer,
}

impl<R> BufferedMarkableReader<R>
where
    R: std::io::Read,
{
    /// Creates a new reader with an unbounded marked buffer and a buffered reader
    /// limited to 8KB by default.
    /// The use of this is very similar to that of the `std::io::BufReader`
    ///
    /// # Example
    // ```
    // //create a new reader
    // let file = std::fs::File::open("path.bin").unwrap();
    // let mut reader = Reader::new(reader);
    // // now use anywhere you would use a standard reader
    // ```
    pub fn new(inner: R) -> BufferedMarkableReader<R> {
        BufferedMarkableReader {
            inner,
            inner_complete: false,
            is_marked: false,
            mark_buffer: Buffer::new(DEFAULT_MARKER_BUFFER_SIZE, None),
            read_buffer: Buffer::new(DEFAULT_BUFFER_SIZE, Some(DEFAULT_BUFFER_SIZE)),
        }
    }

    /// Creates a new reader with an limited marked buffer and a buffered reader
    /// limited to 8KB by default.
    /// Any reads that exceed the provided limit will result in an `std::io::Error(ErrorKind::OutOfMemory)` error
    /// The use of this is very similar to that of the `std::io::BufReader`
    ///
    /// # Example
    // ```
    // //create a new reader
    // let file = std::fs::File::open("path.bin").unwrap();
    // let mut reader = Reader::new_with_limited_back_buffer(reader, 1024 /*1KB back buffer*/);
    // // now use anywhere you would use a standard reader
    // ```
    pub fn new_with_limited_back_buffer(inner: R, limit: usize) -> BufferedMarkableReader<R> {
        BufferedMarkableReader {
            inner,
            inner_complete: false,
            is_marked: false,
            mark_buffer: Buffer::new(DEFAULT_MARKER_BUFFER_SIZE, Some(limit)),
            read_buffer: Buffer::new(DEFAULT_BUFFER_SIZE, Some(DEFAULT_BUFFER_SIZE)),
        }
    }

    /// Creates a new reader using the provided capacities as the initial capacity and limit.
    /// Any reads that exceed the provided limit will result in an `std::io::Error(ErrorKind::OutOfMemory)` error
    /// The use of this is very similar to that of the `std::io::BufReader`
    ///
    /// # Example
    // ```
    // //create a new reader
    // let file = std::fs::File::open("path.bin").unwrap();
    // let mut reader = Reader::new_with_capacity_and_limit(reader, 1024 /*1KB back buffer capacity and limit */, 1024 /* 1KB reader buffer capacity */);
    // // now use anywhere you would use a standard reader
    // ```
    pub fn new_with_capacity_and_limit(
        inner: R,
        back_buffer_capacity: usize,
        reader_buffer_capacity: usize,
    ) -> BufferedMarkableReader<R> {
        BufferedMarkableReader {
            inner,
            inner_complete: false,
            is_marked: false,
            mark_buffer: Buffer::new(back_buffer_capacity, Some(back_buffer_capacity)),
            read_buffer: Buffer::new(reader_buffer_capacity, Some(reader_buffer_capacity)),
        }
    }

    /// Returns the inner reader. **IMPORTANT** this will likely result in data loss
    /// of whatever data has been read into the buffer
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Marks the location of the inner stream. From tis point forward
    /// reads will be cached. If the stream was marked prior to this call
    /// the current buffer will be discarded.
    ///
    /// Returns the number of bytes that were discarded as a result of this operation
    pub fn mark(&mut self) -> usize {
        self.is_marked = true;
        self.mark_buffer.clear()
    }

    /// Resets the stream previously marked position, if it is set.
    /// If the reader was not previously marked, this has no affect.
    pub fn reset(&mut self) {
        self.is_marked = false;
    }

    /// Reads at most `buf.len()` bytes from the underlying buffers to fill the provided buffer.
    fn read_into_buf(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // If marked, then we only read from the read buffer and all
        // read bytes go in the mark buffer.
        // If not marked, we read what we can from the mark buffer and then read the remaining
        // bytes from the read buffer, which may need to be filled.

        if self.is_marked {
            // If marked, just read from internal stream and push to mark buffer
            self.read_data_into_buf_and_marked_stream(buf)
        } else {
            // Otherwise, read what we can from the mark buffer and then go to the read buffer
            // for any remaining bytes
            let mut bytes_read = self.mark_buffer.read_into(buf, 0);
            bytes_read += self.fill_from_read_buffer(buf, bytes_read)?;

            if bytes_read == 0 {
                Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))
            } else {
                Ok(bytes_read)
            }
        }
    }

    /// Fills the provided buffer with bytes from the underlying stream and also places those
    /// bytes into the mark buffer
    fn read_data_into_buf_and_marked_stream(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_read = self.fill_from_read_buffer(buf, 0)?;
        if bytes_read > 0 {
            self.mark_buffer.write_all(buf)?;
            Ok(bytes_read)
        } else {
            Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))
        }
    }

    /// Fills the provided buffer with bytes from the read buffer starting with at the provided offset
    fn fill_from_read_buffer(&mut self, buf: &mut [u8], offset: usize) -> std::io::Result<usize> {
        if self.inner_complete {
            return Ok(0);
        }

        if self.read_buffer.len() < buf.len() {
            match self.fill_read_buffer() {
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.inner_complete = true;
                }
                Err(e) => return Err(e),
                _ => {}
            }
        }

        Ok(self.read_buffer.read_into(buf, offset))
    }

    /// Fills the internal read buffer with bytes from the underlying buffer
    fn fill_read_buffer(&mut self) -> std::io::Result<()> {
        let read_length = self.read_buffer.get_available_space();
        let mut buf = vec![0; read_length];
        let bytes_read = self.inner.read(&mut buf)?;
        self.read_buffer.write_all(&buf[0..bytes_read])?;
        Ok(())
    }
}

impl<R> std::io::Read for BufferedMarkableReader<R>
where
    R: std::io::Read,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read_into_buf(buf)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use super::BufferedMarkableReader;

    #[test]
    fn test_basic_read() {
        let input_data = vec![0, 1, 2, 3];
        let data = Cursor::new(input_data.clone());
        let mut reader = BufferedMarkableReader::new(data);

        let mut read_buf = vec![0; input_data.len()];
        reader
            .read_exact(&mut read_buf)
            .expect("should be able to read bytes back");
        assert_eq!(
            input_data, read_buf,
            "read buffer and input buffer should match"
        );
    }

    #[test]
    fn test_marked_read() {
        let input_data = vec![0, 1, 2, 3];
        let data = Cursor::new(input_data.clone());
        let mut reader = BufferedMarkableReader::new(data);

        let mut single_byte_buf = vec![0];
        reader
            .read_exact(&mut single_byte_buf)
            .expect("should be able to read single byte");

        assert_eq!(0, reader.mark(), "no bytes should be wasted");

        let mut rest_of_buf = vec![0; input_data.len() - 1];
        reader
            .read_exact(&mut rest_of_buf)
            .expect("should be able to read rest of buffer");

        reader.reset();
        rest_of_buf = vec![0; input_data.len() - 1];

        reader
            .read_exact(&mut rest_of_buf)
            .expect("should be able to read rest of buffer again after reset");

        assert_eq!(
            input_data[1..],
            rest_of_buf,
            "buffer should be last 3 bytes"
        );
    }

    #[test]
    fn test_back_buffer_and_read_buffer_read() {
        let input_data = vec![0, 1, 2, 3];
        let data = Cursor::new(input_data.clone());
        let mut reader = BufferedMarkableReader::new(data);

        let mut half_buf = vec![0; input_data.len() / 2];
        reader.mark();
        reader
            .read_exact(&mut half_buf)
            .expect("should be able to read half the buffer");

        reader.reset();
        let mut whole_buf = vec![0; input_data.len()];
        reader
            .read_exact(&mut whole_buf)
            .expect("should be able to whole buffer");

        assert_eq!(
            input_data, whole_buf,
            "input data and whole buf should match"
        );
    }
}
