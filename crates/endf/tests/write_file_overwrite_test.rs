//! Verify the `write_file` / `write_file_overwrite` split matches the
//! Python reference `writefile(..., overwrite=...)` semantics:
//!
//! * `write_file` refuses to clobber an existing file and returns
//!   `EndfError::FileExists`.
//! * `write_file_overwrite` replaces whatever was there.
//!
//! The existence-check uses `OpenOptions::create_new`, so the failure
//! path is atomic with respect to filesystem state (no TOCTOU window).

use endf::error::EndfError;
use endf::parser::EndfParser;
use endf::value::{EndfKey, EndfValue};
use std::path::{Path, PathBuf};

/// RAII guard: removes the target file on drop, tolerating missing
/// targets so that early-aborted tests don't spuriously fail cleanup.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str) -> Self {
        // Use PID and the test name to avoid collisions when running
        // tests in parallel or repeatedly.
        let path = std::env::temp_dir()
            .join(format!("endf_p1_test_{}_{}.endf", std::process::id(), name));
        // Pre-clean in case a previous aborted run left a file behind.
        let _ = std::fs::remove_file(&path);
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Build the smallest valid ENDF-dict payload that `parser.write()`
/// accepts: an MF=0/MT=0 section containing just a TPID string. The
/// resulting file is ~240 bytes, parses back cleanly, and avoids any
/// dependency on external ENDF fixtures.
fn minimal_tpid_data() -> EndfValue {
    let mut tpid_section = EndfValue::new_dict();
    tpid_section.insert(
        EndfKey::Str("TPID".into()),
        EndfValue::Str("write_file_overwrite_test TPID".into()),
    );
    let mut mt_map = EndfValue::new_dict();
    mt_map.insert(EndfKey::Int(0), tpid_section);
    let mut data = EndfValue::new_dict();
    data.insert(EndfKey::Int(0), mt_map);
    data
}

#[test]
fn write_file_refuses_to_clobber_and_overwrite_variant_replaces() {
    let parser = EndfParser::builder().build().expect("builder");
    let data = minimal_tpid_data();
    let tmp = TempFile::new("clobber");
    let path = tmp.path();

    // 1. First call: target does not exist, write_file must succeed.
    parser
        .write_file(path, &data)
        .expect("first write_file on a fresh path must succeed");
    assert!(path.exists(), "file should exist after first write");
    let first_bytes = std::fs::read(path).expect("read after first write");
    assert!(!first_bytes.is_empty(), "first write produced empty file");

    // 2. Second call with the same write_file: target now exists, must
    //    fail with EndfError::FileExists carrying the exact target path.
    match parser.write_file(path, &data) {
        Err(EndfError::FileExists { path: reported }) => {
            assert_eq!(
                reported, path,
                "FileExists.path must echo the target path we supplied"
            );
        }
        Err(other) => panic!("expected FileExists, got {:?}", other),
        Ok(()) => panic!("second write_file unexpectedly succeeded — file was clobbered"),
    }

    // The failed call must not have truncated or rewritten the file.
    // (OpenOptions::create_new guarantees atomicity; this just pins the
    // guarantee in a regression-catching assertion.)
    let bytes_after_fail = std::fs::read(path).expect("read after failed write");
    assert_eq!(
        bytes_after_fail, first_bytes,
        "failed write_file must leave the existing file unchanged"
    );

    // 3. Explicit overwrite: must succeed and replace the contents.
    //    Use a distinct payload so we can observe the replacement.
    let mut data2 = minimal_tpid_data();
    // Replace the TPID string so the bytes differ from data1.
    data2
        .get_mut(EndfKey::Int(0))
        .unwrap()
        .get_mut(EndfKey::Int(0))
        .unwrap()
        .insert(
            EndfKey::Str("TPID".into()),
            EndfValue::Str("write_file_overwrite_test TPID REPLACED".into()),
        );
    parser
        .write_file_overwrite(path, &data2)
        .expect("write_file_overwrite on an existing path must succeed");
    let bytes_after_overwrite =
        std::fs::read(path).expect("read after write_file_overwrite");
    assert_ne!(
        bytes_after_overwrite, first_bytes,
        "write_file_overwrite must replace the existing contents"
    );
    // tmp dropped here → file removed.
}
