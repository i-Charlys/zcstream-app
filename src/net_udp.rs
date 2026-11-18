use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rune::Waker;

use crate::state::{
    AppEvent, NetConnectMode, NetRole, NetSessionStats, ZCSTREAM_BUILD_DIR, StreamCodec,
    ZPSYNC_BUILD_DIR,
};

#[derive(Clone, Debug)]
pub struct ZpSyncLaunchOpts {
    pub secret: String,
    pub peer_ip: String,
    pub peer_port: u16,
    pub local_port: u16,
    pub birthday: bool,
    pub sentinel: bool,
}

fn wake(waker: &Option<Waker>) {
    if let Some(w) = waker {
        w();
    }
}

pub fn probe_zpsync_network(local_port: u16, tx: Sender<AppEvent>, waker: Option<Waker>) {
    thread::spawn(move || {
        let exe_path = Path::new(ZPSYNC_BUILD_DIR).join("zpsync_peer.exe");
        if !exe_path.exists() {
            let _ = tx.send(AppEvent::ZpProbeDone {
                summary: "zpsync_peer.exe introuvable".into(),
                logs: vec![format!("Erreur: introuvable : {}", exe_path.display())],
            });
            wake(&waker);
            return;
        }

        let mut args = vec!["--probe".to_owned()];
        if local_port > 0 {
            args.push("--local-port".to_owned());
            args.push(local_port.to_string());
        }

        let mut logs = vec![format!("$ {} {}", exe_path.display(), args.join(" "))];
        match Command::new(&exe_path)
            .args(&args)
            .current_dir(ZPSYNC_BUILD_DIR)
            .output()
        {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut ipv6 = String::new();
                let mut pub_v4 = String::new();
                let mut nat = String::new();
                for line in stdout.lines() {
                    logs.push(line.to_owned());
                    let t = line.trim();
                    if let Some(rest) = t.strip_prefix("[zpsync] IPv6 available :") {
                        ipv6 = rest.trim().to_owned();
                    } else if let Some(rest) = t.strip_prefix("[zpsync] Public IPv4    :") {
                        pub_v4 = rest.trim().to_owned();
                    } else if let Some(rest) = t.strip_prefix("[zpsync] NAT type       :") {
                        nat = rest.trim().to_owned();
                    }
                }
                for line in stderr.lines() {
                    if !line.trim().is_empty() {
                        logs.push(format!("[stderr] {line}"));
                    }
                }
                let summary = if out.status.success() {
                    format!(
                        "NAT: {} | IPv6: {} | Pub: {}",
                        if nat.is_empty() { "Inconnu" } else { &nat },
                        if ipv6.is_empty() { "?" } else { &ipv6 },
                        if pub_v4.is_empty() { "N/A" } else { &pub_v4 }
                    )
                } else {
                    "Échec du sondage STUN/IPv6".to_owned()
                };
                let _ = tx.send(AppEvent::ZpProbeDone { summary, logs });
            }
            Err(e) => {
                logs.push(format!("Erreur lancement: {e}"));
                let _ = tx.send(AppEvent::ZpProbeDone {
                    summary: format!("Erreur: {e}"),
                    logs,
                });
            }
        }
        wake(&waker);
    });
}

#[allow(clippy::too_many_arguments)]
pub fn start_network_session(
    role: NetRole,
    connect_mode: NetConnectMode,
    codec: StreamCodec,
    width: u32,
    height: u32,
    host: String,
    port: u16,
    zp_opts: Option<ZpSyncLaunchOpts>,
    fps: u32,
    bitrate_mbps: u32,
    fec_shards: u8,
    use_rio: bool,
    tx: Sender<AppEvent>,
    waker: Option<Waker>,
    stop: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let exe_path = Path::new(ZCSTREAM_BUILD_DIR).join("zcrudp_stream.exe");
        if !exe_path.exists() {
            let _ = tx.send(AppEvent::NetStopped(format!(
                "Exécutable introuvable : {}",
                exe_path.display()
            )));
            wake(&waker);
            return;
        }

        let mut args = vec![
            "--role".to_owned(),
            match role {
                NetRole::Sender => "sender".to_owned(),
                NetRole::Receiver => "receiver".to_owned(),
            },
            "--codec".to_owned(),
            codec.as_arg().to_owned(),
            "--width".to_owned(),
            width.to_string(),
            "--height".to_owned(),
            height.to_string(),
        ];

        match connect_mode {
            NetConnectMode::DirectUdp => {
                args.extend(["--port".to_owned(), port.to_string()]);
                if role == NetRole::Sender {
                    args.extend(["--host".to_owned(), host.clone()]);
                }
            }
            NetConnectMode::ZpSync => {
                if let Some(ref zp) = zp_opts {
                    args.extend(["--zpsync".to_owned(), zp.secret.clone()]);
                    if !zp.peer_ip.is_empty() {
                        args.extend(["--zp-peer".to_owned(), zp.peer_ip.clone()]);
                    }
                    if zp.peer_port > 0 {
                        args.extend(["--zp-port".to_owned(), zp.peer_port.to_string()]);
                    }
                    if zp.local_port > 0 {
                        args.extend(["--zp-local-port".to_owned(), zp.local_port.to_string()]);
                    }
                    if zp.birthday {
                        args.push("--zp-birthday".to_owned());
                    }
                    if !zp.sentinel {
                        args.push("--zp-no-sentinel".to_owned());
                    }
                }
            }
        }

        if role == NetRole::Sender {
            args.extend([
                "--fps".to_owned(),
                fps.to_string(),
                "--bitrate".to_owned(),
                bitrate_mbps.to_string(),
                "--fec".to_owned(),
                fec_shards.to_string(),
            ]);
            if !use_rio {
                args.push("--no-rio".to_owned());
            }
        }

        let _ = tx.send(AppEvent::NetLog(format!(
            "$ {} {}",
            exe_path.display(),
            args.join(" ")
        )));
        wake(&waker);

        let mut cmd = Command::new(&exe_path);
        cmd.args(&args)
            .current_dir(ZCSTREAM_BUILD_DIR)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AppEvent::NetStopped(format!("Erreur lancement: {e}")));
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
                    let _ = tx_err.send(AppEvent::NetLog(format!("[stderr] {line}")));
                    wake(&waker_err);
                }
            }
        });

        let tx_out = tx.clone();
        let waker_out = waker.clone();
        let out_handle = thread::spawn(move || {
            let mut stats = NetSessionStats {
                running: true,
                status_msg: match (connect_mode, role) {
                    (NetConnectMode::DirectUdp, NetRole::Sender) => {
                        format!("Émission vers {host}:{port} ({})...", codec.as_arg())
                    }
                    (NetConnectMode::DirectUdp, NetRole::Receiver) => {
                        format!("Réception sur 0.0.0.0:{port} ({})...", codec.as_arg())
                    }
                    (NetConnectMode::ZpSync, NetRole::Sender) => {
                        format!("ZPSync P2P Émetteur ({})...", codec.as_arg())
                    }
                    (NetConnectMode::ZpSync, NetRole::Receiver) => {
                        format!("ZPSync P2P Récepteur ({})...", codec.as_arg())
                    }
                },
                ..Default::default()
            };
            if let Some(out) = stdout {
                let reader = BufReader::new(out);
                for line in reader.lines().map_while(Result::ok) {
                    if parse_stream_line(&mut stats, &line) {
                        let _ = tx_out.send(AppEvent::NetStats(stats.clone()));
                    }
                    let _ = tx_out.send(AppEvent::NetLog(line));
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
                Ok(None) => thread::sleep(Duration::from_millis(30)),
                Err(_) => break -1,
            }
        };

        let _ = out_handle.join();
        let _ = err_handle.join();
        let msg = match exit_code {
            0 | -2 => "Session zcrudp_stream arrêtée.".to_owned(),
            c => format!("zcrudp_stream terminé avec le code {c}."),
        };
        let _ = tx.send(AppEvent::NetStopped(msg));
        wake(&waker);
    });
}

fn parse_stream_line(stats: &mut NetSessionStats, line: &str) -> bool {
    let t = line.trim();
    if let Some(rest) = t.strip_prefix("[ZPSYNC]") {
        let r = rest.trim();
        stats.status_msg = format!("ZPSync : {r}");
        if r.starts_with("Connected!") {
            for seg in r.split('|') {
                let s = seg.trim();
                if let Some(val) = s.strip_prefix("Connected! Path=") {
                    stats.zp_path = val.trim().to_owned();
                } else if let Some(val) = s.strip_prefix("Session=") {
                    stats.zp_session = val.trim().to_owned();
                } else if let Some(val) = s.strip_prefix("RTT=") {
                    if let Some(num) = val.split_whitespace().next().and_then(|n| n.parse::<u32>().ok()) {
                        stats.zp_rtt_ms = Some(num);
                    }
                }
            }
        }
        return true;
    }
    if let Some(rest) = t.strip_prefix("Capture  :") {
        stats.status_msg = format!("Capture : {}", rest.trim());
        return true;
    }
    if let Some(rest) = t.strip_prefix("[SENDER]") {
        // e.g. "120.0 fps | 24.85 Mbps | total=600 frames | pkts=15000 | errs=0"
        for seg in rest.split('|') {
            let s = seg.trim();
            if s.ends_with("fps") {
                if let Some(v) = s.split_whitespace().next().and_then(|n| n.parse::<f64>().ok()) {
                    stats.fps = v;
                }
            } else if s.ends_with("Mbps") {
                if let Some(v) = s.split_whitespace().next().and_then(|n| n.parse::<f64>().ok()) {
                    stats.mbps = v;
                }
            } else if let Some(val) = s.strip_prefix("total=") {
                if let Some(v) = val.split_whitespace().next().and_then(|n| n.parse::<u64>().ok()) {
                    stats.frames_tx = v;
                }
            } else if let Some(val) = s.strip_prefix("pkts=") {
                if let Ok(v) = val.trim().parse::<u64>() {
                    stats.packets_total = v;
                }
            } else if let Some(val) = s.strip_prefix("errs=") {
                if let Ok(v) = val.trim().parse::<u64>() {
                    stats.frames_dropped = v;
                }
            }
        }
        return true;
    }
    if let Some(rest) = t.strip_prefix("[RECEIVER]") {
        // e.g. "120.0 fps | 24.85 Mbps | decoded=600 | fec_rec=0 | dropped=0"
        for seg in rest.split('|') {
            let s = seg.trim();
            if s.ends_with("fps") {
                if let Some(v) = s.split_whitespace().next().and_then(|n| n.parse::<f64>().ok()) {
                    stats.fps = v;
                }
            } else if s.ends_with("Mbps") {
                if let Some(v) = s.split_whitespace().next().and_then(|n| n.parse::<f64>().ok()) {
                    stats.mbps = v;
                }
            } else if let Some(val) = s.strip_prefix("decoded=") {
                if let Ok(v) = val.trim().parse::<u64>() {
                    stats.frames_rx = v;
                }
            } else if let Some(val) = s.strip_prefix("fec_rec=") {
                if let Ok(v) = val.trim().parse::<u64>() {
                    stats.fec_recovered = v;
                }
            } else if let Some(val) = s.strip_prefix("dropped=") {
                if let Ok(v) = val.trim().parse::<u64>() {
                    stats.frames_dropped = v;
                }
            }
        }
        return true;
    }
    false
}
