use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::state::{find_binary, zcstream_dir};

static BOOTSTRAP_LOCK: Mutex<()> = Mutex::new(());

fn tool_in_path(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn ensure_repo_present(repo_name: &str, dir: &Path, log: &mut dyn FnMut(String)) {
    if dir.is_dir() {
        return;
    }
    let url = format!("https://github.com/i-Charlys/{repo_name}.git");
    log(format!("Info: dépôt `{repo_name}` absent localement. Clonage de {url}..."));
    if let Some(parent) = dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if tool_in_path("git") {
        let status = Command::new("git")
            .args(["clone", "--depth", "1", &url])
            .arg(dir)
            .output();
        match status {
            Ok(out) if out.status.success() => {
                log(format!("Info: `{repo_name}` cloné dans {}", dir.display()));
            }
            Ok(out) => {
                log(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
            Err(e) => {
                log(format!("Erreur git clone `{repo_name}`: {e}"));
            }
        }
    }
}

fn run_cmake_configure_and_build(dir: &Path, label: &str, log: &mut dyn FnMut(String)) {
    let build_dir = dir.join("build");
    if !build_dir.join("CMakeCache.txt").is_file() {
        log(format!("Info: configuration CMake pour {label}..."));
        let mut cfg_cmd = Command::new("cmake");
        if cfg!(windows) {
            if tool_in_path("clang") && tool_in_path("mingw32-make") {
                cfg_cmd.args([
                    "-S", ".", "-B", "build", "-G", "MinGW Makefiles",
                    "-DCMAKE_C_COMPILER=clang", "-DCMAKE_CXX_COMPILER=clang++",
                    "-DCMAKE_BUILD_TYPE=Release",
                ]);
            } else if tool_in_path("gcc") && tool_in_path("mingw32-make") {
                cfg_cmd.args([
                    "-S", ".", "-B", "build", "-G", "MinGW Makefiles",
                    "-DCMAKE_C_COMPILER=gcc", "-DCMAKE_CXX_COMPILER=g++",
                    "-DCMAKE_BUILD_TYPE=Release",
                ]);
            } else if tool_in_path("clang") && tool_in_path("ninja") {
                cfg_cmd.args([
                    "-S", ".", "-B", "build", "-G", "Ninja",
                    "-DCMAKE_C_COMPILER=clang", "-DCMAKE_CXX_COMPILER=clang++",
                    "-DCMAKE_BUILD_TYPE=Release",
                ]);
            } else {
                cfg_cmd.args(["-S", ".", "-B", "build"]);
            }
        } else {
            cfg_cmd.args(["-S", ".", "-B", "build", "-DCMAKE_BUILD_TYPE=Release"]);
        }
        if let Ok(out) = cfg_cmd.current_dir(dir).output() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if !line.trim().is_empty() {
                    log(line.to_owned());
                }
            }
            if !out.status.success() {
                for line in String::from_utf8_lossy(&out.stderr).lines() {
                    if !line.trim().is_empty() {
                        log(line.to_owned());
                    }
                }
            }
        }
    }

    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .to_string();
    let bld = Command::new("cmake")
        .args(["--build", "build", "--config", "Release", "-j", &jobs])
        .current_dir(dir)
        .output();
    if let Ok(out) = bld
        && !out.status.success()
    {
        for line in String::from_utf8_lossy(&out.stderr).lines() {
            if !line.trim().is_empty() {
                log(line.to_owned());
            }
        }
    }
}

pub fn ensure_binary_available(
    base_exe: &str,
    mut log: impl FnMut(String),
) -> Option<PathBuf> {
    if let Some(p) = find_binary(base_exe) {
        return Some(p);
    }
    let _guard = BOOTSTRAP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = find_binary(base_exe) {
        return Some(p);
    }
    if !tool_in_path("cmake") {
        return None;
    }
    let dir = zcstream_dir();
    ensure_repo_present("zcstream", &dir, &mut log);
    if dir.is_dir() {
        run_cmake_configure_and_build(&dir, "zcstream/build", &mut log);
    }
    find_binary(base_exe)
}
