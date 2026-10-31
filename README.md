# tally

[samply](https://github.com/mstange/samply) records where a program spends its time and shows it in the Firefox Profiler, a browser view an agent cannot read. Its recording is JSON in a format built for that view, with symbols in a sidecar, so reading it from a terminal means a script over two files and an unstable format.

tally runs the program under samply and prints the recording as two tables: the share of samples each function holds as the innermost frame (self), and the share holding it anywhere on the stack (inclusive).

```bash
tally record --limit 5 -- ./target/release/spin
```

```
recorded at profile.json
samples: 2345
self
 96.5%  spin::collatz_steps
  2.7%  spin::main
  0.5%  _GI_execve
inclusive
 99.3%  _libc_start_call_main
 99.3%  _libc_start_main_alias_2
 99.3%  main
 99.3%  start
 99.3%  std::rt::lang_start_internal
```

`tally summarize profile.json` prints the same tables for a recording already made. Every row above a tenth of a percent prints, the heaviest 100 per table by default; `--limit` sets it, up to 1,000. `tally --help` is the whole grammar.

The inclusive table's first rows are the runtime's: `main` and the frames above it hold every sample of any program, and samply's own child is recorded until it execs the program, which is the `_GI_execve` row. `--under NAME` counts only the samples holding the function NAME on their stack, each cut at its outermost NAME frame, so the tables show where the time under NAME goes, NAME's inclusive row reads 100.0%, and the first line reads `samples: COUNT of RECORDED under NAME`. NAME is a function as the tables print it, so a row of one summary is the scope of the next, and `--under main` leaves samply's child out:

```bash
tally summarize --under spin::collatz_steps profile.json
```

## Recording a Rust program

Build it with its symbols kept, so frames name functions rather than addresses:

```toml
[profile.profiling]
inherits = "release"
debug = "line-tables-only"
strip = "none"
```

On Linux, samply finds a file's symbols by its GNU build-id, and a Rust program linked by Nix's toolchain carries none unless asked: link with `-C link-arg=-Wl,--build-id`, through `RUSTFLAGS` or a `.cargo/config.toml`. A file without one has its frames named for the file alone, and tally names it on stderr.

A frame is named for the symbol whose address range holds it, and the sidecar carries no inline records, so a function the compiler inlined is counted under the function it was inlined into: the `self` share of a small hot function lands on its caller. Under a release profile that is most small functions, and `#[inline(never)]` on one keeps it in the tables.

samply reads perf events on Linux, which an unprivileged user may open only while `kernel.perf_event_paranoid` is 1 or lower, and it maps a 1,028 KiB ring buffer per CPU, which fails with `mmap failed` on a host of many CPUs until `kernel.perf_event_mlock_kb` is at least 1028.

A criterion bench records through cargo's runner:

```bash
cargo bench --profile profiling --bench outline --config "target.'cfg(all())'.runner = ['tally', 'record', '--output', 'target/profiling/outline.json', '--']" -- --profile-time 5 rust/40002
```

## Installing

The flake's package carries the samply its lock pins and puts it ahead of any other on the binary's path, because the symbols sidecar tally reads is a format samply marks unstable and the two move together. `cargo install --locked --path .` installs the binary alone, which runs the `samply` on the path.

## License

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
