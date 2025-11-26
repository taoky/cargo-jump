# cargo-jump

A single command to bump versions in a Cargo workspace based on changed files. I would like to use `cargo-release` but sadly it does not support this simple feature: <https://github.com/crate-ci/cargo-release/issues/298>.

Usage:

```sh
cargo jump 0.20251127.0 --old-tag v0.20251111.0
```

If `--old-tag` is not provided, it defaults to updating all packages in the workspace.

If `--dry-run`, no Cargo.toml files will be modified.
