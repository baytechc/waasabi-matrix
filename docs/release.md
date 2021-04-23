# waasabi-matrix Release Process

The release of `waasabi-matrix` is automated.

## Requirements

* [`cargo-release`](https://crates.io/crates/cargo-release)
  * Install using `cargo install cargo-release`

## Release process

1. Add any notable changes to `CHANGELOG.md` under the `Unreleased changes` header
2. Run `cargo release <level-or-version>`
   * Choose `patch` if this release contains only bug fixes
   * Choose `minor` if this release contains some new features
   * Choose `major` if this release contains breaking changes


## Resources

* [semver.org](https://semver.org/)
