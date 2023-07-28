/// Creates a buffer with an initial capacity and optional limit
#[derive(Debug, PartialEq)]
pub(crate) struct Buffer {
    pos: usize,
    size: usize,
    buffer_limit: Option<usize>,
    buffer: Vec<u8>,
}

impl Buffer {
    /// Creates a new buffer with the provided initial capacity and optional limit.
    pub fn new(buffer_size: usize, buffer_limit: Option<usize>) -> Buffer {
        Buffer {
            pos: 0,
            size: 0,
            buffer_limit,
            buffer: Vec::with_capacity(buffer_size),
        }
    }

    /// Clears the buffer and returns how many bytes were dropped
    pub fn clear(&mut self) -> usize {
        let dropped = self.buffer.len() - self.pos;
        self.pos = 0;
        self.buffer.clear();
        dropped
    }

    /// Reads values from this buffer into the provided `buf`.
    /// Returns the number of bytes placed in the provided `buf`
    pub fn read_into(&mut self, buf: &mut [u8], offset: usize) -> usize {
        let requested_byte_count = buf.len() - offset.min(buf.len());
        let bytes_to_read = self.buffer.len().min(requested_byte_count);

        for i in 0..bytes_to_read {
            buf[i + offset] = self.buffer[i + self.pos];
        }

        self.pos += bytes_to_read;
        bytes_to_read
    }

    /// Appends a slice into the buffer.
    /// If a buffer limit has been imposed and this will
    /// exceed that limit, an out of memory error will be returned.
    fn append(&mut self, buf: &[u8]) -> std::io::Result<()> {
        if self.size_exceeds_capacity(buf.len()) {
            return Err(std::io::Error::from(std::io::ErrorKind::OutOfMemory));
        }

        self.prepare_for_bytes(buf.len());
        self.buffer.extend(buf);
        Ok(())
    }

    /// Determines if a byte size will exceed the limit, if set, of this buffer
    fn size_exceeds_capacity(&self, size: usize) -> bool {
        let used_space = self.len();

        self.buffer_limit
            .map(|limit| (used_space + size) > limit)
            .unwrap_or(false)
    }

    /// Gets the length of the unread bytes in the buffer
    pub fn len(&self) -> usize {
        self.buffer.len() - self.pos
    }

    /// Gets the available space within the buffer that is available without
    /// resizing the underlying buffer
    pub fn get_available_space(&self) -> usize {
        (self.buffer.capacity() - self.buffer.len()) + self.pos
    }

    /// Prepares the internal buffer to receive data of the provided size
    /// If the provided size is larger than the available space, previously
    /// read elements are removed and vec is shifted left for the new elements
    /// to be appended
    fn prepare_for_bytes(&mut self, byte_size: usize) {
        // Available space is the data the buffer can hold, less current amount of
        // data in the buffer, plus the current position which represents read
        // (i.e., now available) space
        let available_space = self.get_available_space();
        if byte_size > available_space {
            let _ = self.buffer.drain(0..self.pos);
            self.pos = 0; // Reset the position
        }
    }
}

impl std::io::Read for Buffer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(self.read_into(buf, 0))
    }
}

impl std::io::Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.append(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::Buffer;

    #[test]
    fn test_simple_read() {
        let mut buffer = Buffer::new(10, None);
        let values: Vec<u8> = vec![0, 1, 2, 3, 4];
        buffer.write_all(&values).unwrap();

        let mut read_buf = vec![0];

        for (p, v) in values.iter().enumerate() {
            buffer.read_exact(&mut read_buf).unwrap();
            assert_eq!(
                read_buf[0], *v,
                "value at [{p}] should be {v}, but was {}",
                read_buf[0]
            );
        }
    }

    #[test]
    fn test_exceeding_limit() {
        let mut buffer = Buffer::new(2, Some(2));
        let values = vec![0, 1, 2];

        match buffer.write_all(&values) {
            Err(err) => {
                assert_eq!(
                    std::io::ErrorKind::OutOfMemory,
                    err.kind(),
                    "should have had an out of memory error"
                );
            }
            _ => {
                panic!("should have failed")
            }
        }
    }

    #[test]
    fn test_reusing_space() {
        let mut buffer = Buffer::new(2, Some(2));
        let mut values = vec![0];
        buffer.write_all(&values).unwrap();

        buffer
            .read(&mut values)
            .expect("should be able to read value");
        values = vec![0, 1];
        buffer
            .write_all(&values)
            .expect("should now have space for 2 values");
        buffer
            .read_exact(&mut values)
            .expect("should be able to read two values back");
        assert_eq!(vec![0, 1], values, "values should be [0, 1]");
    }

    #[test]
    fn test_dynamic_growing() {
        let mut buffer = Buffer::new(2, None);
        let values = vec![0, 1, 2, 3];
        buffer.write_all(&values).expect("with no limit imposed, the internal buffer should grow to accomodate additional capacity");
    }

    #[test]
    fn test_use_after_clear() {
        let mut buffer = Buffer::new(2, Some(5));
        let values = vec![0, 1, 2, 3];
        buffer
            .write_all(&values)
            .expect("should be able to write 4 items");

        assert_eq!(
            values.len(),
            buffer.clear(),
            "should have dropped all values",
        );

        buffer
            .write_all(&values)
            .expect("should be able to write 4 items");
        let mut read_buffer = vec![0; 4];
        buffer
            .read_exact(&mut read_buffer)
            .expect("should be able to fill the buffer with values");

        assert_eq!(
            values, read_buffer,
            "values and read buffer should be identical"
        );
    }
}
