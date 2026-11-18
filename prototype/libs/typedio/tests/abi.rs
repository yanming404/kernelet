// SPDX-License-Identifier: MPL-2.0
use std::{
    fs::DirBuilder,
    mem::{align_of, offset_of, size_of},
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use typedio::{
    abi::ReservationRecord,
    notification::Binding,
    service::{Call, Descriptor, Record, Resource, ABI_VERSION},
};

struct TestDirectory(PathBuf);
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn actual_c_and_rust_records_have_identical_layout() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/typedio_abi.c");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("typedio-abi-{}-{nonce}", std::process::id()));
    // Atomic directory creation rejects preexisting paths and mode 0700
    // prevents another account from substituting compiler output paths.
    DirBuilder::new().mode(0o700).create(&path).unwrap();
    let directory = TestDirectory(path);
    let output = directory.0.join("probe");
    let compiled = Command::new("gcc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
        .arg(root)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let result = Command::new(&output).output().unwrap();
    assert!(result.status.success());
    let expected = format!(
        "{},{},{},{},{},{}\n",
        size_of::<ReservationRecord>(),
        align_of::<ReservationRecord>(),
        offset_of!(ReservationRecord, token),
        offset_of!(ReservationRecord, offset),
        offset_of!(ReservationRecord, length),
        offset_of!(ReservationRecord, progress)
    );
    let service = format!(
        "{},{},{},{},{},{},{},{},{},{},{}\n",
        size_of::<Descriptor>(),
        offset_of!(Descriptor, config),
        size_of::<Record>(),
        offset_of!(Record, resources),
        offset_of!(Record, resource_count),
        size_of::<Call>(),
        offset_of!(Call, descriptor),
        size_of::<Resource>(),
        offset_of!(Resource, kind),
        size_of::<Binding>(),
        ABI_VERSION
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        expected + &service
    );
}
