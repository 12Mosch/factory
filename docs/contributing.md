# Contributing

Install Rust 1.98.1 with the `rustfmt` and `clippy` components using
[rustup](https://rustup.rs/). On Linux, install the [Bevy build dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md).

Run the same quality gates used by GitHub Actions from the repository root:

```sh
cargo fmt --all
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

After formatting, check `git diff` for any changes to commit. The `Rust quality`
job runs on pull requests and pushes to `main`, and can be selected as a required
check in branch protection.
