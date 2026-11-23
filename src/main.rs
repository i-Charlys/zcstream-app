mod net_udp;
mod runner;
mod state;
mod views;

use rune::{AppSpec, Cx, El, Tx, Waker};

use crate::runner::parse_bench_line;
use crate::state::{AppEvent, AppState, TestStatus};

fn setup(s: &mut AppState, waker: Waker) {
    s.waker = Some(waker);
}

fn poll(s: &mut AppState, _t: f64, _tx: &mut Tx) -> bool {
    let mut changed = false;
    while let Ok(ev) = s.rx.try_recv() {
        changed = true;
        match ev {
            AppEvent::BenchLog(line) => {
                parse_bench_line(&mut s.bench_results, &line);
                if s.bench_logs.len() >= 500 {
                    s.bench_logs.remove(0);
                }
                s.bench_logs.push(line);
            }
            AppEvent::BenchDone { exit_code } => {
                s.bench_running = false;
                s.bench_exit_code = Some(exit_code);
                s.bench_stop = None;
            }
            AppEvent::TestLog { id, line } => {
                if let Some(entry) = s.tests.iter_mut().find(|t| t.id == id) {
                    if entry.logs.len() >= 500 {
                        entry.logs.remove(0);
                    }
                    entry.logs.push(line);
                }
            }
            AppEvent::TestDone { id, passed, duration_ms } => {
                if let Some(entry) = s.tests.iter_mut().find(|t| t.id == id) {
                    entry.status = if passed {
                        TestStatus::Passed
                    } else {
                        TestStatus::Failed
                    };
                    entry.duration_ms = Some(duration_ms);
                }
            }
            AppEvent::NetStats(stats) => {
                s.net_stats = stats;
            }
            AppEvent::NetLog(line) => {
                if s.net_logs.len() >= 300 {
                    s.net_logs.remove(0);
                }
                s.net_logs.push(line);
            }
            AppEvent::NetStopped(msg) => {
                s.net_stats.running = false;
                s.net_stats.status_msg = msg;
                s.net_stop = None;
            }
            AppEvent::ZpProbeDone { summary, logs } => {
                s.zp_probing = false;
                s.zp_probe_info = Some(summary);
                for line in logs {
                    if s.net_logs.len() >= 300 {
                        s.net_logs.remove(0);
                    }
                    s.net_logs.push(line);
                }
            }
        }
    }
    changed
}

fn view(s: &AppState, _cx: &mut Cx<AppState>) -> El {
    let k = rune_theme_opencode::kit(s.dark);
    rune_kit::root(views::app_view(s, k), Vec::new())
}

fn main() {
    rune::run(AppSpec::<AppState> {
        setup: Some(setup),
        poll: Some(poll),
        scenes: &[
            ("tests", |s| s.tab = crate::state::Tab::Tests),
            ("network", |s| s.tab = crate::state::Tab::Network),
        ],
        ..AppSpec::new("zcstream — RUDP Control Center", (1200.0, 820.0), AppState::default, view)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn tabs_switch_and_render_cleanly() {
        let env = Env { system_dark: true, scale: 1.0 };
        let mut rt = Runtime::new(AppState::default(), view, (1200.0, 820.0), env);
        rt.frame(0.0);
        assert_eq!(rt.state.tab, crate::state::Tab::Benchmarks);
        assert!(rt.find("btn_run_bench").is_some());
        assert!(rt.find("bench_hz_480").is_some());
        assert!(rt.find("bench_fps_input").is_some());
        assert!(rt.find("bench_bitrate_auto").is_some());
        assert!(rt.find("bench_bitrate_seg").is_some());
        assert!(rt.find("bench_bitrate_input").is_some());

        // Auto bandwidth scales with resolution & fps; min_mbps gates high Hz when Mbps is low.
        assert!(AppState::min_mbps_for_mode(1920, 1080, 480) > 25);
        assert!(AppState::min_mbps_for_mode(1920, 1080, 120) <= 15);
        assert!(AppState::min_mbps_for_mode(3840, 2160, 60) > 15);
        assert!(AppState::auto_bitrate_mbps(1920, 1080, 480) >= 65);
        assert!(AppState::auto_bitrate_mbps(3840, 2160, 60) >= 40);

        let custom = AppState {
            bench_fps_edit: rune::TextEdit::new("360"),
            bench_bitrate_edit: rune::TextEdit::new("145"),
            ..AppState::default()
        };
        let args = custom.build_bench_args();
        assert!(args.windows(2).any(|w| w == ["--fps", "360"]));
        assert!(args.windows(2).any(|w| w == ["--bitrate", "145000000"]));

        let pyro_bench = AppState {
            pipeline: crate::state::PipelineKind::Pyrowave,
            bench_fps_edit: rune::TextEdit::new("240"),
            bench_bitrate_edit: rune::TextEdit::new("60"),
            use_rio: false,
            ..AppState::default()
        };
        assert_eq!(
            crate::state::PipelineKind::Pyrowave.exe_name(),
            crate::state::bin_name("zcrudp_pyrowave_bench")
        );
        let pyro_args = pyro_bench.build_bench_args();
        assert!(pyro_args.windows(2).any(|w| w == ["--fps", "240"]));
        assert!(pyro_args.windows(2).any(|w| w == ["--target-bytes", "31250"]));
        assert_eq!(
            pyro_args.iter().any(|a| a == "--no-rio"),
            crate::state::host_supports_rio()
        );
        let mut rt_pyro_bench = Runtime::new(pyro_bench, view, (1200.0, 820.0), env);
        rt_pyro_bench.frame(0.0);
        assert!(rt_pyro_bench.find("btn_run_bench").is_some());

        let tests_state = AppState {
            tab: crate::state::Tab::Tests,
            ..AppState::default()
        };
        let mut rt_tests = Runtime::new(tests_state, view, (1200.0, 820.0), env);
        rt_tests.frame(0.0);
        assert!(rt_tests.find("btn_run_test_3").is_some());

        let net_state = AppState {
            tab: crate::state::Tab::Network,
            net_connect_mode: crate::state::NetConnectMode::ZpSync,
            net_role: crate::state::NetRole::Receiver,
            net_codec: crate::state::StreamCodec::Pyrowave,
            ..AppState::default()
        };
        assert_eq!(crate::state::StreamCodec::Pyrowave.as_arg(), "pyrowave");
        let mut rt_net = Runtime::new(net_state, view, (1200.0, 820.0), env);
        rt_net.frame(0.0);
        assert!(rt_net.find("btn_zp_probe").is_some());
        assert!(rt_net.find("zp_secret_input").is_some());
        assert!(rt_net.find("net_codec_seg").is_some());
        assert!(rt_net.find("net_hz_480").is_some());
        assert!(rt_net.find("net_fps_input").is_some());

        let net_sender = AppState {
            tab: crate::state::Tab::Network,
            net_role: crate::state::NetRole::Sender,
            net_codec: crate::state::StreamCodec::Pyrowave,
            net_bitrate_edit: rune::TextEdit::new("15"),
            ..AppState::default()
        };
        let mut rt_sender = Runtime::new(net_sender, view, (1200.0, 820.0), env);
        rt_sender.frame(0.0);
        assert!(rt_sender.find("net_codec_seg").is_some());
        assert!(rt_sender.find("net_hz_480").is_some());
        assert!(rt_sender.find("net_fps_input").is_some());
        assert!(rt_sender.find("net_bitrate_auto").is_some());
        assert!(rt_sender.find("net_bitrate_seg").is_some());
        assert!(rt_sender.find("net_bitrate_input").is_some());

        // Platform agnosticism checks (OS/arch detection + universal Zcvid defaults)
        assert!(!crate::state::host_platform_badge().is_empty());
        assert!(crate::state::PipelineKind::Zcvid.is_supported_on_platform());
        assert!(crate::state::StreamCodec::Zcvid.is_supported_on_platform());
        assert_eq!(
            crate::state::bin_name("zcrudp_stream"),
            format!("zcrudp_stream{}", std::env::consts::EXE_SUFFIX)
        );
        assert_eq!(
            AppState::default().pipeline,
            crate::state::PipelineKind::Zcvid
        );
    }

    #[test]
    fn binary_and_repo_discovery_is_portable() {
        let base = std::env::temp_dir().join(format!(
            "zcstream_app_disc_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let exe_dir = base.join("portable");
        let root = base.join("ws");
        let msvc_rel = root.join("zcstream").join("build").join("Release");
        let deps_zcvid = root.join("zcstream").join("build").join("_deps").join("zcvid-src");
        let env_dir = base.join("override");

        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&msvc_rel).unwrap();
        std::fs::create_dir_all(&deps_zcvid).unwrap();
        std::fs::create_dir_all(&env_dir).unwrap();

        let stream_name = crate::state::bin_name("zcrudp_stream");
        let msvc_bin = msvc_rel.join(&stream_name);
        std::fs::write(&msvc_bin, b"stub").unwrap();

        // 1. Multi-config MSVC Release/ subdir is discovered automatically
        assert_eq!(
            crate::state::find_binary_in("zcrudp_stream", None, Some(&exe_dir), &root, None),
            Some(msvc_bin.clone())
        );

        // 2. Portable bundle next to executable takes priority over workspace build dir
        let portable_bin = exe_dir.join(&stream_name);
        std::fs::write(&portable_bin, b"stub").unwrap();
        assert_eq!(
            crate::state::find_binary_in("zcrudp_stream", None, Some(&exe_dir), &root, None),
            Some(portable_bin.clone())
        );

        // 3. Explicit ZCSTREAM_BIN_DIR overrides everything
        let override_bin = env_dir.join(&stream_name);
        std::fs::write(&override_bin, b"stub").unwrap();
        assert_eq!(
            crate::state::find_binary_in("zcrudp_stream", Some(&env_dir), Some(&exe_dir), &root, None),
            Some(override_bin)
        );

        // 4. Missing binary returns None cleanly
        assert_eq!(
            crate::state::find_binary_in("nonexistent_tool_xyz", None, Some(&exe_dir), &root, None),
            None
        );

        // 5. Repo discovery falls back to CMake auto-fetched _deps/<name>-src
        assert_eq!(
            crate::state::find_repo_dir_in("zcvid", &root, Some(&exe_dir)),
            deps_zcvid
        );

        // 6. GitHub ZIP download (<repo>-main) takes priority over _deps
        let zip_zcvid = root.join("zcvid-main");
        std::fs::create_dir_all(&zip_zcvid).unwrap();
        assert_eq!(
            crate::state::find_repo_dir_in("zcvid", &root, Some(&exe_dir)),
            zip_zcvid
        );

        // 7. Direct sibling repo in workspace root takes priority over <repo>-main and _deps
        let sibling_zcvid = root.join("zcvid");
        std::fs::create_dir_all(&sibling_zcvid).unwrap();
        assert_eq!(
            crate::state::find_repo_dir_in("zcvid", &root, Some(&exe_dir)),
            sibling_zcvid
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
