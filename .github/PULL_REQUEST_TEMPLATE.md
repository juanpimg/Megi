## Summary

<!-- What does this PR do? Link to the issue if applicable. -->

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes (0 failures)
- [ ] `cargo run -p test_harness -- --summary` passes (gate PASS)
- [ ] Goldens byte-identical (if applicable)
- [ ] `cargo doc --workspace --no-deps` builds
- [ ] No new `unsafe` without `// SAFETY:` comment
- [ ] Commit messages follow conventional format
- [ ] Documentation updated (if applicable)

## Testing

<!-- How was this tested? -->

## Notes

<!-- Any additional context -->
