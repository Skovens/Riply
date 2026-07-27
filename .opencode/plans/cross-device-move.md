# Task 4: Cross-Device Move Fallback

## Status: DONE

## Problem

`app/src/mover.rs:55` uses `fs::rename()` to move files. This fails
silently with an error when source and destination are on different
filesystems (e.g., ext4 scan dir → NTFS mount). The error message is
unhelpful.

## Fix

Detect the `CrossesDevices` error kind and fall back to copy + delete:

```rust
match fs::rename(&file.path, &dest_path) {
    Ok(()) => {}
    Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
        info!("Cross-device move, falling back to copy+delete");
        fs::copy(&file.path, &dest_path)?;
        fs::remove_file(&file.path)?;
    }
    Err(e) => return Err(e.into()),
}
```

### Considerations

- `fs::copy` works across filesystems but doesn't preserve metadata
  (permissions, timestamps). For media files this is fine.
- For large files (4K rips, 50GB+), copy could take a while. The job
  scheduler already runs this in a background task with cancellation
  support, so no extra work needed.
- After successful copy, verify the copy size matches the original before
  deleting the source.

## Files

| File | Change |
|------|--------|
| `app/src/mover.rs` | Lines 54-55: add cross-device fallback |

## Tests

Add a test that creates two temp directories on the same filesystem,
then mocks a cross-device scenario by checking the error handling path.
Or simply test that `move_file` works when src and dest are on the same
fs (existing behavior) and add a unit test for the fallback branch.

## Verification

- `cargo test --all`
- Manual: move a file across filesystems if possible
