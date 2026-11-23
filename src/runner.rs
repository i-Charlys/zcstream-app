use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rune::Waker;

use crate::state::{
    AppEvent, BenchResults, PipelineKind, StatRow, TestTargetId, find_binary, zcrudp_dir,
    zcstream_dir, zcvid_dir, zpsync_dir,
};

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
        let Some(exe_path) = find_binary(pipeline.base_exe_name()) else {
            let _ = tx.send(AppEvent::BenchLog(format!(
                "Erreur: exécutable `{}` introuvable. Lancez ./install.ps1 (Windows) ou ./install.sh (Linux/macOS), ou définissez ZCSTREAM_BIN_DIR.",
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

fn ensure_cmake_built(
    id: TestTargetId,
    dir: &Path,
    label: &str,
    tx: &Sender<AppEvent>,
    waker: &Option<Waker>,
) {
    let build_dir = dir.join("build");
    if build_dir.join("CTestTestfile.cmake").is_file() {
        return;
    }
    let _ = tx.send(AppEvent::TestLog {
        id,
        line: format!("Info: {label} n'existe pas encore. Configuration CMake..."),
    });
    wake(waker);
    let mut cfg_cmd = Command::new("cmake");
    if cfg!(windows) {
        if tool_in_path("clang") && tool_in_path("mingw32-make") {
            cfg_cmd.args([
                "-S", ".", "-B", "build", "-G", "MinGW Makefiles",
                "-DCMAKE_C_COMPILER=clang", "-DCMAKE_CXX_COMPILER=clang++",
            ]);
        } else if tool_in_path("gcc") && tool_in_path("mingw32-make") {
            cfg_cmd.args([
                "-S", ".", "-B", "build", "-G", "MinGW Makefiles",
                "-DCMAKE_C_COMPILER=gcc", "-DCMAKE_CXX_COMPILER=g++",
            ]);
        } else {
            cfg_cmd.args(["-S", ".", "-B", "build"]);
        }
    } else {
        cfg_cmd.args(["-S", ".", "-B", "build"]);
    }
    let cfg = cfg_cmd.current_dir(dir).output();
    if let Ok(out) = cfg {
        let _ = tx.send(AppEvent::TestLog {
            id,
            line: String::from_utf8_lossy(&out.stdout).to_string(),
        });
        if !out.status.success() {
            let _ = tx.send(AppEvent::TestLog {
                id,
                line: String::from_utf8_lossy(&out.stderr).to_string(),
            });
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
        let _ = tx.send(AppEvent::TestLog {
            id,
            line: String::from_utf8_lossy(&out.stderr).to_string(),
        });
    }
}

fn ctest_args_for(dir: &Path) -> Vec<String> {
    let mut args = vec![
        "--test-dir".into(),
        "build".into(),
        "--output-on-failure".into(),
    ];
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
                ensure_cmake_built(id, &dir, "zcstream/build", &tx, &waker);
                let args = ctest_args_for(&dir);
                ("ctest", args, dir)
            }
            TestTargetId::ZcvidCodec => {
                let dir = zcvid_dir();
                if cfg!(windows) && tool_in_path("mingw32-make") && (tool_in_path("clang") || tool_in_path("gcc")) {
                    let cc = if tool_in_path("clang") {
                        "CC=clang"
                    } else {
                        "CC=gcc"
                    };
                    ("mingw32-make", vec![cc.into(), "test".into()], dir)
                } else if !cfg!(windows) && tool_in_path("make") {
                    ("make", vec!["test".into()], dir)
                } else {
                    ensure_cmake_built(id, &dir, "zcvid/build", &tx, &waker);
                    let args = ctest_args_for(&dir);
                    ("ctest", args, dir)
                }
            }
            TestTargetId::ZcrudpCore => {
                let dir = zcrudp_dir();
                ensure_cmake_built(id, &dir, "zcrudp/build", &tx, &waker);
                let args = ctest_args_for(&dir);
                ("ctest", args, dir)
            }
            TestTargetId::Zpsync => {
                let dir = zpsync_dir();
                ensure_cmake_built(id, &dir, "zpsync/build", &tx, &waker);
                let args = ctest_args_for(&dir);
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
