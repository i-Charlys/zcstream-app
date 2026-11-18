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

        let mut tests_state = AppState::default();
        tests_state.tab = crate::state::Tab::Tests;
        let mut rt_tests = Runtime::new(tests_state, view, (1200.0, 820.0), env);
        rt_tests.frame(0.0);
        assert!(rt_tests.find("btn_run_test_3").is_some());

        let mut net_state = AppState::default();
        net_state.tab = crate::state::Tab::Network;
        net_state.net_connect_mode = crate::state::NetConnectMode::ZpSync;
        let mut rt_net = Runtime::new(net_state, view, (1200.0, 820.0), env);
        rt_net.frame(0.0);
        assert!(rt_net.find("btn_zp_probe").is_some());
        assert!(rt_net.find("zp_secret_input").is_some());
    }
}
