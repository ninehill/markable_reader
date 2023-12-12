use std::io::Write;

use super::{buffer::Buffer, MarkerStream, DEFAULT_MARKER_BUFFER_SIZE};

/// Reads bytes from the inner source with the additional ability
/// to `mark` a stream at a point that can be returned to later
/// using the a call to `reset()`. Whlie the stream is marked all
/// subsequent reads are returned as usual, but are also buffered,
///
/// which is what allows for returning to a previous part of the
/// the stream.
///
/// If the inner stream should also be buffered, use `BufferedMarkableStream`,
/// which may offer a slight optimization over passing a `std::io::BufReader`
/// as the inner reader to this stream.
pub struct MarkableReader<R> {
    inner: R,
    inner_complete: bool,
    is_marked: bool,
    mark_buffer: Buffer,
}

impl<R> MarkableReader<R>
where
    R: std::io::Read,
{
    /// Creates a new reader with an unbounded marked buffer
    ///
    /// # Example
    // ```
    // //create a new reader
    // let file = std::fs::File::open("path.bin").unwrap();
    // let mut reader = MarkableReader::new(reader);
    // // now use anywhere you would use a standard reader
    // ```
    pub fn new(inner: R) -> MarkableReader<R> {
        MarkableReader {
            inner,
            inner_complete: false,
            is_marked: false,
            mark_buffer: Buffer::new(DEFAULT_MARKER_BUFFER_SIZE, None),
        }
    }

    /// Creates a new reader with an limited marked buffer
    /// Any reads that exceed the provided limit will result in an `std::io::Error(ErrorKind::OutOfMemory)` error
    /// The use of this is very similar to that of the `std::io::BufReader`
    ///
    /// # Example
    // ```
    // //create a new reader
    // let file = std::fs::File::open("path.bin").unwrap();
    // let mut reader = MarkableReader::new_with_limited_back_buffer(reader, 1024 /*1KB back buffer*/);
    // // now use anywhere you would use a standard reader
    // ```
    pub fn new_with_limited_back_buffer(inner: R, limit: usize) -> MarkableReader<R> {
        MarkableReader {
            inner,
            inner_complete: false,
            is_marked: false,
            mark_buffer: Buffer::new(DEFAULT_MARKER_BUFFER_SIZE, Some(limit)),
        }
    }

    /// Creates a new reader using the provided capacities as the initial capacity and limit.
    /// Any reads that exceed the provided limit will result in an `std::io::Error(ErrorKind::OutOfMemory)` error
    ///
    /// # Example
    // ```
    // //create a new reader
    // let file = std::fs::File::open("path.bin").unwrap();
    // let mut reader = MarkableReader::new_with_capacity_and_limit(reader, 1024 /*1KB back buffer capacity and limit */, 1024 /* 1KB reader buffer capacity */);
    // // now use anywhere you would use a standard reader
    // ```
    pub fn new_with_capacity_and_limit(
        inner: R,
        capacity: usize,
        limit: usize,
    ) -> MarkableReader<R> {
        MarkableReader {
            inner,
            inner_complete: false,
            is_marked: false,
            mark_buffer: Buffer::new(capacity, Some(limit)),
        }
    }

    /// Returns the inner reader. **IMPORTANT** this will likely result in data loss
    /// of whatever data has been read into the buffer
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Reads at most `buf.len()` bytes from the underlying buffers to fill the provided buffer.
    fn read_into_buf(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // If marked, then we only read from the read buffer and all
        // read bytes go in the mark buffer.
        // If not marked, we read what we can from the mark buffer and then read the remaining
        // bytes from the underlying reader.
        if self.is_marked {
            // First grab what we can from the mark buffer
            let buffer_bytes_read = self.mark_buffer.read_into(buf, 0);
            // Then fill and retain remaining from the inner reader
            let inner_bytes_read =
                self.read_data_into_buf_and_marked_stream(buf, buffer_bytes_read)?;
            Ok(inner_bytes_read + buffer_bytes_read)
        } else {
            // Otherwise, read what we can from the mark buffer and then go to inner reader
            // for any remaining bytes
            let mut bytes_read = self.mark_buffer.read_into(buf, 0);
            bytes_read += self.fill_from_inner(buf, bytes_read)?;

            if bytes_read == 0 {
                Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))
            } else {
                Ok(bytes_read)
            }
        }
    }

    /// Fills the provided buffer with bytes from the underlying stream and also places those
    /// bytes into the mark buffer
    fn read_data_into_buf_and_marked_stream(
        &mut self,
        buf: &mut [u8],
        offset: usize,
    ) -> std::io::Result<usize> {
        let inner_bytes_read = self.fill_from_inner(buf, offset)?;
        if inner_bytes_read > 0 {
            // Inner the inner bytes read will be last n bytes that were read from into the buffer
            let inner_bytes = &buf[buf.len() - inner_bytes_read..buf.len()];
            self.mark_buffer.write(inner_bytes)?;
        }

        Ok(inner_bytes_read)
    }

    /// Fills the provided buffer with bytes from the read buffer starting with at the provided offset
    fn fill_from_inner(&mut self, buf: &mut [u8], offset: usize) -> std::io::Result<usize> {
        if self.inner_complete {
            return Ok(0);
        }

        let mut read = 0;
        let mut single_byte_buf = vec![0; 1];
        while read + offset < buf.len() {
            let current_read = self.inner.read(&mut single_byte_buf)?;
            if current_read > 0 {
                buf[read + offset] = single_byte_buf[0];
                read += 1;
            } else {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
            }
        }

        Ok(read)
    }
}

impl<R> MarkerStream for MarkableReader<R> {
    /// Marks the location of the inner stream. From tis point forward
    /// reads will be cached. If the stream was marked prior to this call
    /// the current buffer will be discarded.
    ///
    /// Returns the number of bytes that were discarded as a result of this operation
    fn mark(&mut self) -> usize {
        self.is_marked = true;
        self.mark_buffer.purge_read()
    }

    /// Resets the stream previously marked position, if it is set.
    /// If the reader was not previously marked, this has no affect.
    ///
    fn reset(&mut self) {
        self.is_marked = false;
        self.mark_buffer.restart();
    }

    fn clear_buffer(&mut self) {
        self.is_marked = false;
        self.mark_buffer.clear();
    }
}

impl<R> std::io::Read for MarkableReader<R>
where
    R: std::io::Read,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read_into_buf(buf)
    }
}

impl<R> From<R> for MarkableReader<R>
where
    R: std::io::Read,
{
    fn from(value: R) -> Self {
        MarkableReader::new(value)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use crate::io::MarkerStream;

    use super::MarkableReader;

    #[test]
    fn test_basic_read() {
        let input_data = vec![0, 1, 2, 3];
        let data = Cursor::new(input_data.clone());
        let mut reader = MarkableReader::new(data);

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
        let mut reader = MarkableReader::new(data);

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
        let mut reader = MarkableReader::new(data);

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

    #[test]
    fn test_read_with_popping_bytes() {
        let input_data = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let data = Cursor::new(input_data.clone());
        let mut reader = MarkableReader::new(data);
        let mut single_byte_buffer = vec![0_u8; 1];

        for i in 0..input_data.len() - 1 {
            reader.mark();
            let expected = input_data[i..i + 2].to_vec();
            let mut actual = [0_u8; 2];
            reader
                .read_exact(&mut actual)
                .expect("should always be able to read 2 bytes");
            assert_eq!(
                expected, actual,
                "bytes at index {i} should be {expected:?} but were {actual:?}"
            );

            reader.reset();
            reader
                .read_exact(&mut single_byte_buffer)
                .expect("should be able to read single byte");
            assert_eq!(
                single_byte_buffer[0], input_data[i],
                "popped byte at index {i} should be {i} but was {}",
                single_byte_buffer[0]
            );
        }
    }
}
