# Contributing

Thanks for taking a look. This project is small, so the workflow is informal:

1. Open an issue or describe the change in a pull request.
2. Keep the change focused; avoid unrelated reformatting.
3. Run the gates before pushing:

   ```bash
   cargo fmt --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test
   ```

4. If you change the CLI, output formats, or the SVG subset, update `README.md`
   and `AGENTS.md` in the same commit.

New dependencies should be justified in the pull request. Generated-output
format changes need a test update (including `tests/golden/U+0041.svg` if the
SVG format changes).

By contributing, you agree that your contributions are licensed under the
project's MIT OR Apache-2.0 terms.
