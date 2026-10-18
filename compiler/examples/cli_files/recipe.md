# Read a file from a CLI

Decode command-line flags into a record and count a selected file's bytes.

`Cli.parse` infers `Args` from the binding. `Path` validates the relative path;
`Fs.rooted(Dir.subdir(...))` narrows access to `data` before reading. The manifest
permits reads only below `./data`. Counting bytes avoids treating file contents as
trusted instructions or decoding them unnecessarily. This small recipe reads the
whole file; use `Fs.with_file` and bounded `Fs.read_chunk` for large inputs.

`CliError` and `FsError` propagate to the CLI's failure reporter. Tests replace
arguments and files with `Cli.of` and `Fs.memory`, checking success, missing flags
and a missing file. The harness also runs `sakura run -- --input note.txt` against
the committed data file and expects `4 bytes`.
