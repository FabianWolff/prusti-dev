#![feature(nll)]

extern crate prusti_common;
extern crate walkdir;

use prusti_common::driver_utils;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;

fn main() {
    if let Err(code) = process(std::env::args().skip(1).collect()) {
        std::process::exit(code);
    }
}

fn process(args: Vec<String>) -> Result<(), i32> {
    let mut prusti_driver_path = std::env::current_exe()
        .expect("current executable path invalid")
        .with_file_name("prusti-filter-driver");
    if cfg!(windows) {
        prusti_driver_path.set_extension("exe");
    }

    let prusti_sysroot =
        prusti_sysroot().expect(&format!("Failed to find Rust's sysroot for Prusti"));

    let compiler_lib = prusti_sysroot.join("lib");

    let prusti_home = prusti_driver_path
        .parent()
        .expect("Failed to find Prusti's home");

    let prusti_contracts_lib = find_prusti_contracts(&prusti_home)
        .expect("Failed to find prusti_contracts library in Prusti's home");

    let mut cmd = Command::new(&prusti_driver_path);
    cmd.args(args);
    cmd.env("SYSROOT", &prusti_sysroot);
    cmd.env("PRUSTI_CONTRACTS_LIB", &prusti_contracts_lib);

    let mut libs = vec![compiler_lib];
    if let Some(target) = option_env!("TARGET") {
        let rustlib_path = prusti_sysroot
            .join("lib")
            .join("rustlib")
            .join(target)
            .join("lib");
        libs.push(rustlib_path);
    }
    driver_utils::add_to_loader_path(libs, &mut cmd);

    let exit_status = cmd.status().expect("could not run prusti-filter-driver");

    if exit_status.success() {
        Ok(())
    } else {
        Err(exit_status.code().unwrap_or(-1))
    }
}

/// Find Prusti's sysroot
fn prusti_sysroot() -> Option<PathBuf> {
    Command::new("rustup")
        .arg("run")
        .arg(include_str!("../../rust-toolchain").trim())
        .arg("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .ok()
        .and_then(|out| {
            print!("{}", String::from_utf8(out.stderr).ok().unwrap());
            String::from_utf8(out.stdout).ok()
        })
        .map(|s| PathBuf::from(s.trim().to_owned()))
}

/// Find the prusti-contracts library
fn find_prusti_contracts<S: AsRef<Path>>(path: S) -> Option<PathBuf> {
    let walker = walkdir::WalkDir::new(path).follow_links(true);

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_e) => continue,
        };

        let file_name = entry.file_name().to_str().unwrap_or("");
        let extension = entry
            .path()
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("");

        if extension == "rlib" && file_name.starts_with("libprusti_contracts") {
            return Some(entry.path().into());
        }
    }

    None
}
