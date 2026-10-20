# Read a file line by line with a stream

Count the lines of a log file in fixed-size reads, with every bound stated.

`Fs.file_source(file, 4_096)` is a view over the open `File`: each pull reads up to 4 KiB.
`Encoding.decode_source` turns those bytes into text, holding a UTF-8 sequence that a read
split until the next chunk completes it, and `lines(1_024)` splits the text on LF wherever
the chunks break, dropping a CR before an LF. `fold` walks the lines once. Memory is bounded
by the chunk and the line limit, not the file: a line over 1,024 bytes fails
`StreamError(MaxLine)` instead of growing, and a malformed byte fails `EncodingError`.

A source is a value. `summarize_text` takes any `Source<String, e>`, so a test hands it a
pure source built with `Source.from_list`, which reads the same items every time it is
pulled. The file view advances the file and is valid only inside the `Fs.with_file`
callback: returning it from the callback is `V6003`. The tests use `Fs.memory`, so they
need no permissions.

A rotated log is often gzipped. `summarize_gz` puts `compress.decompress_source` between
the file and the decoder: the codec state lives only inside its callback, the compressed
bytes are read 4 KiB at a time, and `max_out` (64 MiB here) bounds what the file may expand
to, so a small file that inflates without end fails `CompressionError(OutputLimit)` instead
of filling memory. A cut-off file fails `Truncated` and bytes that are not gzip fail
`Malformed(offset)`.
