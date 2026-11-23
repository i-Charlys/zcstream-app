use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, atomic::AtomicBool};

use rune::{TextEdit, Waker};

pub fn workspace_root() -> PathBuf {
    if let Some(root) = std::env::var_os("ZCSTREAM_ROOT") {
        let p = PathBuf::from(root);
        if p.exists() {
            return p;
        }
    }
    let has_repos = |p: &Path| {
        [
            "zcstream",
            "zcstream-main",
            "zcvid",
            "zcvid-main",
            "zcrudp",
            "zcrudp-main",
            "zpsync",
            "zpsync-main",
            "zcstream-app",
            "zcstream-app-main",
        ]
        .iter()
        .any(|d| p.join(d).is_dir())
    };
    let is_single_repo_root = |p: &Path| {
        (p.join("Cargo.toml").is_file() || p.join("src").join("stream_live.c").is_file())
            && !p.join("zcstream").is_dir()
    };
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors().skip(1) {
            if has_repos(anc) {
                return anc.to_path_buf();
            }
            if is_single_repo_root(anc)
                && let Some(parent) = anc.parent()
            {
                return parent.to_path_buf();
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for anc in cwd.ancestors() {
            if has_repos(anc) {
                return anc.to_path_buf();
            }
            if is_single_repo_root(anc)
                && let Some(parent) = anc.parent()
            {
                return parent.to_path_buf();
            }
        }
        return cwd;
    }
    PathBuf::from(".")
}

pub fn find_repo_dir_in(name: &str, root: &Path, exe_dir: Option<&Path>) -> PathBuf {
    for suffix in ["", "-main", "-master"] {
        let cand = root.join(format!("{name}{suffix}"));
        if cand.is_dir() {
            return cand;
        }
    }
    if let Some(ed) = exe_dir {
        for suffix in ["", "-main", "-master"] {
            let beside = ed.join(format!("{name}{suffix}"));
            if beside.is_dir() {
                return beside;
            }
        }
    }
    for zc_name in ["zcstream", "zcstream-main", "zcstream-master"] {
        let fetched = root
            .join(zc_name)
            .join("build")
            .join("_deps")
            .join(format!("{name}-src"));
        if fetched.is_dir() {
            return fetched;
        }
        let nested = root.join(zc_name).join("deps").join(name);
        if nested.is_dir() {
            return nested;
        }
    }
    root.join(name)
}

pub fn find_repo_dir(name: &str) -> PathBuf {
    let root = workspace_root();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    find_repo_dir_in(name, &root, exe_dir.as_deref())
}

pub fn zcstream_dir() -> PathBuf {
    find_repo_dir("zcstream")
}

pub fn bin_name(base: &str) -> String {
    format!("{base}{}", std::env::consts::EXE_SUFFIX)
}

pub fn find_binary_in(
    base: &str,
    env_bin_dir: Option<&Path>,
    exe_dir: Option<&Path>,
    root: &Path,
    path_env: Option<&OsStr>,
) -> Option<PathBuf> {
    let name = bin_name(base);
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(dir) = env_bin_dir {
        dirs.push(dir.to_path_buf());
    }
    if let Some(ed) = exe_dir {
        dirs.push(ed.to_path_buf());
        dirs.push(ed.join("bin"));
    }

    dirs.push(root.join("dist"));

    for build_base in [
        root.join("zcstream").join("build"),
        root.join("zcstream-main").join("build"),
        root.join("zpsync").join("build"),
        root.join("zpsync-main").join("build"),
        root.join("zcstream-app").join("zcstream").join("build"),
    ] {
        dirs.push(build_base.clone());
        dirs.push(build_base.join("Release"));
        dirs.push(build_base.join("RelWithDebInfo"));
        dirs.push(build_base.join("Debug"));
    }

    for dir in dirs {
        let candidate = dir.join(&name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            let candidate = dir.join(&name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

pub fn find_binary(base: &str) -> Option<PathBuf> {
    let env_bin_dir = std::env::var_os("ZCSTREAM_BIN_DIR").map(PathBuf::from);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let root = workspace_root();
    let path_env = std::env::var_os("PATH");
    find_binary_in(
        base,
        env_bin_dir.as_deref(),
        exe_dir.as_deref(),
        &root,
        path_env.as_deref(),
    )
}

pub fn host_os_label() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "linux" => "Linux",
        "macos" => "macOS",
        other => other,
    }
}

pub fn host_arch_label() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "ARM64",
        other => other,
    }
}

pub fn host_platform_badge() -> String {
    format!("{} {}", host_os_label(), host_arch_label())
}

pub fn host_supports_nvenc() -> bool {
    cfg!(all(target_os = "windows", target_arch = "x86_64"))
}

pub fn host_supports_rio() -> bool {
    cfg!(target_os = "windows")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolutionPreset {
    pub label: &'static str,
    pub width: u32,
    pub height: u32,
}

pub const RESOLUTIONS: &[ResolutionPreset] = &[
    ResolutionPreset { label: "1280×720", width: 1280, height: 720 },
    ResolutionPreset { label: "1920×1080", width: 1920, height: 1080 },
    ResolutionPreset { label: "2560×1440", width: 2560, height: 1440 },
    ResolutionPreset { label: "3840×2160", width: 3840, height: 2160 },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetRole {
    Sender,
    Receiver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetConnectMode {
    DirectUdp,
    ZpSync,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamCodec {
    Nvenc,
    Pyrowave,
    Zcvid,
}

impl StreamCodec {
    pub fn as_arg(self) -> &'static str {
        match self {
            StreamCodec::Nvenc => "nvenc",
            StreamCodec::Pyrowave => "pyrowave",
            StreamCodec::Zcvid => "zcvid",
        }
    }

    pub fn is_supported_on_platform(self) -> bool {
        match self {
            StreamCodec::Nvenc => host_supports_nvenc(),
            StreamCodec::Pyrowave | StreamCodec::Zcvid => true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NetSessionStats {
    pub running: bool,
    pub fps: f64,
    pub mbps: f64,
    pub frames_tx: u64,
    pub frames_rx: u64,
    pub fec_recovered: u64,
    pub frames_dropped: u64,
    pub packets_total: u64,
    pub status_msg: String,
    pub zp_path: String,
    pub zp_session: String,
    pub zp_rtt_ms: Option<u32>,
}

pub enum AppEvent {
    NetStats(NetSessionStats),
    NetLog(String),
    NetStopped(String),
    ZpProbeDone { summary: String, logs: Vec<String> },
}

pub struct AppState {
    pub dark: bool,
    pub waker: Option<Waker>,
    pub tx: Sender<AppEvent>,
    pub rx: Receiver<AppEvent>,

    // Network streaming state
    pub net_role: NetRole,
    pub net_connect_mode: NetConnectMode,
    pub net_codec: StreamCodec,
    pub net_res_idx: Option<usize>,
    pub net_width_edit: TextEdit,
    pub net_height_edit: TextEdit,
    pub net_host_edit: TextEdit,
    pub net_port_edit: TextEdit,
    pub zp_secret_edit: TextEdit,
    pub zp_peer_ip_edit: TextEdit,
    pub zp_peer_port_edit: TextEdit,
    pub zp_local_port_edit: TextEdit,
    pub zp_birthday: bool,
    pub zp_sentinel: bool,
    pub zp_probing: bool,
    pub zp_probe_info: Option<String>,
    pub net_fps: u32,
    pub net_fps_edit: TextEdit,
    pub net_bitrate_mbps: u32,
    pub net_bitrate_edit: TextEdit,
    pub net_fec_shards: u8,
    pub net_use_rio: bool,
    pub net_stop: Option<Arc<AtomicBool>>,
    pub net_stats: NetSessionStats,
    pub net_logs: Vec<String>,
}

impl Default for AppState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let net_host_edit = TextEdit::new("127.0.0.1");
        let net_port_edit = TextEdit::new("9900");
        let net_width_edit = TextEdit::new("1920");
        let net_height_edit = TextEdit::new("1080");
        let net_fps_edit = TextEdit::new("120");
        let net_bitrate_edit = TextEdit::new("");
        let zp_secret_edit = TextEdit::new("zcstream-p2p");
        let zp_peer_ip_edit = TextEdit::new("");
        let zp_peer_port_edit = TextEdit::new("0");
        let zp_local_port_edit = TextEdit::new("0");

        Self {
            dark: true,
            waker: None,
            tx,
            rx,
            net_role: NetRole::Sender,
            net_connect_mode: NetConnectMode::DirectUdp,
            net_codec: StreamCodec::Zcvid,
            net_res_idx: Some(1),
            net_width_edit,
            net_height_edit,
            net_host_edit,
            net_port_edit,
            zp_secret_edit,
            zp_peer_ip_edit,
            zp_peer_port_edit,
            zp_local_port_edit,
            zp_birthday: false,
            zp_sentinel: true,
            zp_probing: false,
            zp_probe_info: None,
            net_fps: 120,
            net_fps_edit,
            net_bitrate_mbps: 50,
            net_bitrate_edit,
            net_fec_shards: 1,
            net_use_rio: host_supports_rio(),
            net_stop: None,
            net_stats: NetSessionStats {
                status_msg: format!(
                    "Prêt à lancer {} ({}).",
                    bin_name("zcrudp_stream"),
                    host_platform_badge()
                ),
                ..Default::default()
            },
            net_logs: Vec::new(),
        }
    }
}

impl AppState {
    /// Recommended auto bandwidth (Mbps) from pixel rate (matches zcrudp_stream).
    pub fn auto_bitrate_mbps(width: u32, height: u32, fps: u32) -> u32 {
        let px_per_sec = width as u64 * height as u64 * fps.max(1) as u64;
        let auto = 20 + px_per_sec / 20_000_000;
        (auto as u32).clamp(25, 200)
    }

    /// Minimum bandwidth (Mbps) required to sustain a given resolution and refresh rate cleanly.
    pub fn min_mbps_for_mode(width: u32, height: u32, fps: u32) -> u32 {
        let px_per_sec = width as u64 * height as u64 * fps.max(1) as u64;
        let mbps = 3 + px_per_sec.div_ceil(25_000_000);
        (mbps as u32).max(4)
    }

    pub fn effective_net_dims(&self) -> (u32, u32) {
        let w = self.net_width_edit.text.trim().parse::<u32>().unwrap_or(1920).max(64);
        let h = self.net_height_edit.text.trim().parse::<u32>().unwrap_or(1080).max(64);
        (w, h)
    }

    pub fn effective_net_fps(&self) -> u32 {
        self.net_fps_edit
            .text
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|&v| v > 0)
            .unwrap_or(self.net_fps)
            .clamp(1, 1000)
    }

    pub fn is_net_bitrate_auto(&self) -> bool {
        let raw = self.net_bitrate_edit.text.trim();
        raw.is_empty() || raw == "0"
    }

    pub fn effective_net_bitrate_mbps(&self) -> u32 {
        if self.is_net_bitrate_auto() {
            return 0; // auto (computed by zcrudp_stream or auto_bitrate_mbps)
        }
        self.net_bitrate_edit
            .text
            .trim()
            .parse::<u32>()
            .unwrap_or(self.net_bitrate_mbps)
            .min(1000)
    }
}
