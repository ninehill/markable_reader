# Markable Stream
A `Markable Stream` functions as an ordinary reader with the added ability to mark a stream at an arbitrary location that can be returned to after subsequent reads. There are two variants in this package: 1) `Markable Stream`; and 2) `Buffered Markable Stream`.

## Usage
Usage is the same as with any other `std::io::Read` trait, with the exception of two additional functions: `mark()` and `reset()`, which mark the location of the stream to return to at a later point and reset the stream back to that position respectively.
