// SPDX-License-Identifier: MPL-2.0
//! Compile the actual native facade and require the specific borrow-check error.
#[test]
fn cpu_local_reference_cannot_outlive_its_guard() {
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let library = std::fs::read_dir(&deps)
        .unwrap()
        .map(|p| p.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("libkernelet_platform_harness-")
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .expect("compiled platform facade");
    let dir = std::env::temp_dir().join(format!("klet-cpu-local-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("escape.rs");
    std::fs::write(
        &input,
        r#"
use kernelet_platform_harness::{cpu,task};
fn main(){
    let local=cpu::local::CpuLocal::new([0usize;8]);
    let escaped={let guard=task::disable_preempt();local.get_with(&guard)};
    assert_eq!(*escaped,0);
}
"#,
    )
    .unwrap();
    let output = std::process::Command::new("rustc")
        .arg("--edition=2021")
        .arg(&input)
        .arg("--extern")
        .arg(format!("kernelet_platform_harness={}", library.display()))
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--out-dir")
        .arg(&dir)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success() && stderr.contains("E0597") && stderr.contains("guard"),
        "{stderr}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
