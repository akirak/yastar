# 1. Use Rust

Date: 2024-11-08

## Status

accepted

## Context and Problem Statement

Choose a programming language (and framework) to fetch data from the GitHub API
and feed it into a DuckDB database.

## Considered Options

- TypeScript
- Rust
- Go

## Decision Outcome

Choose Rust, because it has a strong type system, reliable error handling, [a
working DuckDB integration](https://duckdb.org/docs/api/rust). Its WASM support
is also a plus, because I plan on running this application on browser.

<!-- This is an optional element. Feel free to remove. -->
### Consequences

* Good, because its initial development was mostly smooth, even with FFI usage.
* Bad, because it was harder to pick a chart library than expected. The lifetime
  system can be a struggle when dealing with multiple series of data. It's even
  impossible to apply a function on the same object multiple times by naively
  looping over an iterator. The library API needs to be designed to handle this
  situation.
* … <!-- numbers of consequences can vary -->
