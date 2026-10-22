# Contributing

Thanks for helping. Bug reports, ideas and pull requests are all welcome. Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).

## Reporting bugs and asking for features

Use the [issue forms](https://github.com/ivadim/pdf-tools/issues/new/choose). For a bug, a PDF that shows the problem helps most (only if it has nothing private in it). Security problems go through [private reporting](SECURITY.md) instead.

## Making changes

1. Set up the build as in [Develop](README.md#develop): Bazelisk is the only prerequisite.
2. Make the change on a branch, with tests for new behaviour (see [Testing](README.md#testing)).
3. Before pushing, run:

   ```bash
   bazel test //...
   bazel build --config=rustfmt //...   # or: bazel run @rules_rust//:rustfmt
   bazel build --config=clippy //...
   (cd web && npm ci && npm run lint)   # if you changed web/
   ```

4. Open a pull request. CI runs the tests on Windows, Linux and macOS, the lints, the security checks and coverage (at least 80% of lines); the **CI passed** check must be green before merging. Pull requests are squash-merged, so the title becomes the commit message.

### Dependencies

Add or update crates in the crates' `Cargo.toml` files; `Cargo.lock` and `MODULE.bazel.lock` must be updated with them (`bazel build --lockfile_mode=update //...`). New crates must have a licence allowed in `deny.toml`.

### Workflows

Actions are pinned to full commit SHAs with the version in a comment; Dependabot keeps them up to date. Give each job only the permissions it needs. CI checks workflows with [zizmor](https://docs.zizmor.sh).

## Licence

By contributing, you agree that your contributions are licensed under the [MIT licence](LICENSE).
