use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rune::Waker;

use crate::state::{
    AppEvent, BenchResults, PipelineKind, StatRow, TestTargetId, find_binary, zcrudp_dir,
    zcstream_dir, zcvid_dir, zpsync_dir,
};

static BOOTSTRAP_LOCK: Mutex<()> = Mutex::new(());

fn wake(waker: &Option<Waker>) {
    if let Some(w) = waker {
        w();
    }
}

pub fn spawn_benchmark(
    pipeline: PipelineKind,
    args: Vec<String>,
    tx: Sender<AppEvent>,
    waker: Option<Waker>,
    stop: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let tx_log = tx.clone();
        let waker_log = waker.clone();
        let Some(exe_path) = ensure_binary_available(pipeline.base_exe_name(), move |line| {
            let _ = tx_log.send(AppEvent::BenchLog(line));
            wake(&waker_log);
        }) else {
            let _ = tx.send(AppEvent::BenchLog(format!(
                "Erreur: exécutable `{}` introuvable et la compilation CMake a échoué. Lancez ./install.ps1 (Windows) ou ./install.sh (Linux/macOS), ou définissez ZCSTREAM_BIN_DIR.",
                pipeline.exe_name()
            )));
            wake(&waker);
            let _ = tx.send(AppEvent::BenchDone { exit_code: -1 });
            wake(&waker);
            return;
        };
        let work_dir = exe_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let _ = tx.send(AppEvent::BenchLog(format!(
            "$ {} {}",
            exe_path.display(),
            args.join(" ")
        )));
        wake(&waker);

        let mut cmd = Command::new(&exe_path);
        cmd.args(&args)
            .current_dir(&work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AppEvent::BenchLog(format!("Erreur lancement: {e}")));
                wake(&waker);
                let _ = tx.send(AppEvent::BenchDone { exit_code: -1 });
                wake(&waker);
                return;
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let tx_err = tx.clone();
        let waker_err = waker.clone();
        let err_handle = thread::spawn(move || {
            if let Some(err) = stderr {
                let reader = BufReader::new(err);
                for line in reader.lines().map_while(Result::ok) {
                    let _ = tx_err.send(AppEvent::BenchLog(format!("[stderr] {line}")));
                    wake(&waker_err);
                }
            }
        });

        let tx_out = tx.clone();
        let waker_out = waker.clone();
        let out_handle = thread::spawn(move || {
            if let Some(out) = stdout {
                let reader = BufReader::new(out);
                for line in reader.lines().map_while(Result::ok) {
                    let _ = tx_out.send(AppEvent::BenchLog(line));
                    wake(&waker_out);
                }
            }
        });

        let exit_code = loop {
            if stop.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                break -2;
            }
            match child.try_wait() {
                Ok(Some(status)) => break status.code().unwrap_or(-1),
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(_) => break -1,
            }
        };

        let _ = out_handle.join();
        let _ = err_handle.join();
        let _ = tx.send(AppEvent::BenchDone { exit_code });
        wake(&waker);
    });
}

pub fn parse_bench_line(res: &mut BenchResults, line: &str) {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("GPU Adapter   :") {
        res.adapter = rest.trim().to_owned();
    } else if let Some(rest) = trimmed.strip_prefix("Video Encoder :") {
        res.encoder = rest.trim().to_owned();
    } else if let Some(rest) = trimmed.strip_prefix("Frames Sent    :") {
        // e.g. "240 | Delivered & Decoded: 240 (100.0%)"
        if let Some((left, right)) = rest.split_once('|') {
            res.frames_sent = left.trim().parse().ok();
            if let Some(after) = right.split(':').nth(1) {
                let parts: Vec<&str> = after.split_whitespace().collect();
                if let Some(first) = parts.first() {
                    res.frames_delivered = first.parse().ok();
                }
                if let Some(pct_tok) = parts.get(1) {
                    let clean = pct_tok.trim_matches(|c| c == '(' || c == ')' || c == '%');
                    res.delivered_pct = clean.parse().ok();
                }
            }
        }
    } else if let Some(rest) = trimmed.strip_prefix("Measured frames:") {
        // e.g. "200 / 260 (FEC recovered: 0 | dropped: 0)"
        let nums: Vec<&str> = rest.split_whitespace().collect();
        if nums.len() >= 3 {
            res.frames_delivered = nums[0].parse().ok();
            res.frames_sent = nums[2].parse().ok();
            if res.frames_sent.unwrap_or(0) > 0 && res.frames_delivered.is_some() {
                res.delivered_pct = Some(100.0);
            }
        }
        if let Some(fec_pos) = rest.find("FEC recovered:") {
            let sub = &rest[fec_pos + "FEC recovered:".len()..];
            if let Some(tok) = sub.split_whitespace().next() {
                res.fec_recovered = tok.trim_end_matches('|').parse().ok();
            }
        }
        if let Some(drop_pos) = rest.find("dropped:") {
            let sub = &rest[drop_pos + "dropped:".len()..];
            if let Some(tok) = sub.split_whitespace().next() {
                res.frames_dropped = tok.trim_end_matches(')').parse().ok();
            }
        }
    } else if let Some(rest) = trimmed.strip_prefix("Network TX     :") {
        for seg in rest.split('|') {
            let s = seg.trim();
            if let Some(num) = s.split_whitespace().next().and_then(|v| v.parse::<u64>().ok()) {
                if s.contains("sent") && !s.contains("FEC") {
                    res.pkts_sent = Some(num);
                } else if s.contains("dropped") {
                    res.pkts_dropped_sim = Some(num);
                } else if s.contains("reordered") {
                    res.pkts_reordered = Some(num);
                } else if s.contains("dup") {
                    res.pkts_dup = Some(num);
                } else if s.contains("FEC") {
                    res.pkts_fec = Some(num);
                } else if s.contains("errors") {
                    res.send_errors = Some(num);
                }
            }
        }
    } else if let Some(rest) = trimmed.strip_prefix("Network RX     :") {
        for seg in rest.split('|') {
            let s = seg.trim();
            if let Some(num) = s.split_whitespace().next().and_then(|v| v.parse::<u64>().ok()) {
                if s.contains("FEC-recovered") {
                    res.fec_recovered = Some(num);
                } else if s.contains("dropped") {
                    res.frames_dropped = Some(num);
                }
            }
        }
    } else if let Some(rest) = trimmed.strip_prefix("Avg bitrate    :") {
        // e.g. "24.85 Mbps (25.9 KB/frame)"
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if let Some(mbps) = parts.first().and_then(|v| v.parse::<f64>().ok()) {
            res.avg_mbps = Some(mbps);
        }
        if let Some(kb) = parts.get(2).and_then(|v| v.trim_start_matches('(').parse::<f64>().ok()) {
            res.kb_per_frame = Some(kb);
        }
    } else if let Some(rest) = trimmed.strip_prefix("Y-PSNR (VRAM)") {
        if let Some((_, val_part)) = rest.split_once("avg:")
            && let Some(num) = val_part.split_whitespace().next().and_then(|v| v.parse::<f64>().ok())
        {
            res.y_psnr_db = Some(num);
        }
    } else if let Some(rest) = trimmed.strip_prefix("Enc/Dec reference") {
        if let Some((_, val_part)) = rest.split_once('|')
            && let Some(num) = val_part.split_whitespace().next().and_then(|v| v.parse::<u64>().ok())
        {
            res.ref_mismatch_px = Some(num);
        }
    } else if trimmed.contains("min:") && trimmed.contains("p50:") && trimmed.contains("p99:") {
        // Stat row: "1. Capture+NV12    | min:  123.4 us | p50:  200.1 us | p95: ... | p99: ... | avg: ..."
        let parts: Vec<&str> = trimmed.split('|').collect();
        if parts.len() >= 6 {
            let label = parts[0].trim().to_owned();
            let parse_us = |seg: &str| -> Option<f64> {
                seg.split(':').nth(1)?.split_whitespace().next()?.parse::<f64>().ok()
            };
            if let (Some(min_us), Some(p50_us), Some(p95_us), Some(p99_us), Some(avg_us)) = (
                parse_us(parts[1]),
                parse_us(parts[2]),
                parse_us(parts[3]),
                parse_us(parts[4]),
                parse_us(parts[5]),
            ) {
                res.rows.push(StatRow { label, min_us, p50_us, p95_us, p99_us, avg_us });
            }
        }
    }
}

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

fn ensure_repo_and_cmake_built(
    id: TestTargetId,
    repo_name: &str,
    dir: &Path,
    label: &str,
    tx: &Sender<AppEvent>,
    waker: &Option<Waker>,
) {
    let _guard = BOOTSTRAP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut log = |line: String| {
        let _ = tx.send(AppEvent::TestLog { id, line });
        wake(waker);
    };
    ensure_repo_present(repo_name, dir, &mut log);
    if dir.is_dir() {
        run_cmake_configure_and_build(dir, label, &mut log);
    }
}

fn ctest_args_for(dir: &Path, verbose: bool) -> Vec<String> {
    let mut args = vec![
        "--test-dir".into(),
        "build".into(),
        "--output-on-failure".into(),
    ];
    if verbose {
        args.push("-V".into());
    }
    if dir.join("build").join("Release").is_dir() {
        args.push("-C".into());
        args.push("Release".into());
    }
    args
}

pub fn spawn_test(id: TestTargetId, tx: Sender<AppEvent>, waker: Option<Waker>) {
    thread::spawn(move || {
        let start = Instant::now();
        let (program, args, cwd): (&str, Vec<String>, PathBuf) = match id {
            TestTargetId::StreamingBridge => {
                let dir = zcstream_dir();
                ensure_repo_and_cmake_built(id, "zcstream", &dir, "zcstream/build", &tx, &waker);
                let args = ctest_args_for(&dir, false);
                ("ctest", args, dir)
            }
            TestTargetId::ZcvidCodec => {
                let dir = zcvid_dir();
                if tool_in_path("cmake") && tool_in_path("ctest") {
                    ensure_repo_and_cmake_built(id, "zcvid", &dir, "zcvid/build", &tx, &waker);
                    let args = ctest_args_for(&dir, true);
                    ("ctest", args, dir)
                } else if cfg!(windows) && tool_in_path("mingw32-make") && (tool_in_path("clang") || tool_in_path("gcc")) {
                    let cc = if tool_in_path("clang") {
                        "CC=clang"
                    } else {
                        "CC=gcc"
                    };
                    ("mingw32-make", vec![cc.into(), "test".into()], dir)
                } else {
                    ("make", vec!["test".into()], dir)
                }
            }
            TestTargetId::ZcrudpCore => {
                let dir = zcrudp_dir();
                ensure_repo_and_cmake_built(id, "zcrudp", &dir, "zcrudp/build", &tx, &waker);
                let args = ctest_args_for(&dir, false);
                ("ctest", args, dir)
            }
            TestTargetId::Zpsync => {
                let dir = zpsync_dir();
                ensure_repo_and_cmake_built(id, "zpsync", &dir, "zpsync/build", &tx, &waker);
                let args = ctest_args_for(&dir, false);
                ("ctest", args, dir)
            }
        };

        if !cwd.is_dir() {
            let _ = tx.send(AppEvent::TestLog {
                id,
                line: format!(
                    "Dépôt source introuvable ({}). Lancez ./install.ps1 ou ./install.sh pour installer l'espace de travail complet.",
                    cwd.display()
                ),
            });
            wake(&waker);
            let _ = tx.send(AppEvent::TestDone {
                id,
                passed: false,
                duration_ms: start.elapsed().as_millis() as u64,
            });
            wake(&waker);
            return;
        }

        let mut cmd = Command::new(program);
        cmd.args(&args)
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AppEvent::TestLog {
                    id,
                    line: format!("Impossible de lancer `{program}`: {e}"),
                });
                wake(&waker);
                let _ = tx.send(AppEvent::TestDone {
                    id,
                    passed: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                wake(&waker);
                return;
            }
        };

        if let Some(out) = child.stdout.take() {
            let reader = BufReader::new(out);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx.send(AppEvent::TestLog { id, line });
                wake(&waker);
            }
        }
        if let Some(err) = child.stderr.take() {
            let reader = BufReader::new(err);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx.send(AppEvent::TestLog { id, line });
                wake(&waker);
            }
        }

        let passed = child.wait().map(|s| s.success()).unwrap_or(false);
        let _ = tx.send(AppEvent::TestDone {
            id,
            passed,
            duration_ms: start.elapsed().as_millis() as u64,
        });
        wake(&waker);
    });
}
