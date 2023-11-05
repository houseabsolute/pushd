# pushd

A simple library for temporarily changing the current working directory.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
pushd = "0.0.1"
```

## Example

```rust
use anyhow::Result
use std::path::PathBuf;
use pushd::Pushd;

fn main() -> Result<()> {
    write_in_etc()?;
    // Current working directory is whatever it was before the call to
    // `write_in_etc`.
}

fn write_in_etc() -> Result<()> {
    let path = PathBuf::new("/etc");
    let _pd = Pushd::new(path)?;
    // Current working directory is now /etc
    //
    // Do something in /etc.
}
```
