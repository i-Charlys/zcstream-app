use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rune::{El, ScrollSpec, TextEdit, col, el, row, spacer, text};
use rune_kit::{
    Badge, BadgeVariant, Button, ButtonVariant, ControlSize, Divider, Kit, SegmentedControl,
    Spinner, Switch, TextInput,
};

use crate::runner::spawn_benchmark;
use crate::state::{
    AppState, BenchResults, PipelineKind, RESOLUTIONS, host_supports_rio,
};
use crate::views::{kpi_box, panel, stepper};

pub fn view(s: &AppState, k: &Kit) -> El {
    let pipe_idx = match s.pipeline {
        PipelineKind::Nvenc => 0,
        PipelineKind::Pyrowave => 1,
        PipelineKind::Zcvid => 2,
    };

    let pipe_supported = s.pipeline.is_supported_on_platform();
    let pipe_present = s.pipeline.is_binary_present();
    let (pipe_status_label, pipe_status_var) = if !pipe_supported {
        ("Indisponible sur cet OS/arch", BadgeVariant::Neutral)
    } else if pipe_present {
        ("Disponible", BadgeVariant::Accent)
    } else {
        ("Auto-build au lancement", BadgeVariant::Subtle)
    };

    let pipeline_bar = panel(
        k,
        "pipeline_panel",
        "Pipeline Vidéo + zcrudp::nextgen",
        Some(s.pipeline.summary()),
        row()
            .w_full()
            .center_y()
            .gap(12.0)
            .child(
                SegmentedControl::new(
                    &[
                        PipelineKind::Nvenc.label(),
                        PipelineKind::Pyrowave.label(),
                        PipelineKind::Zcvid.label(),
                    ],
                    Some(pipe_idx),
                )
                .fit()
                .on_pick(|s: &mut AppState, idx, _| {
                    s.pipeline = match idx {
                        0 => PipelineKind::Nvenc,
                        1 => PipelineKind::Pyrowave,
                        _ => PipelineKind::Zcvid,
                    };
                })
                .build(k)
                .key("pipe_seg"),
            )
            .child(spacer())
            .child(
                Badge::new(pipe_status_label)
                    .variant(pipe_status_var)
                    .build(k),
            )
            .child(
                Badge::new(s.pipeline.exe_name())
                    .variant(BadgeVariant::Neutral)
                    .build(k),
            ),
    );

    let res = RESOLUTIONS[s.res_idx];
    let cur_fps = s.effective_bench_fps();
    let is_auto_bw = s.is_bench_bitrate_auto();
    let cur_mbps = s.effective_bench_bitrate_mbps();
    let auto_mbps = AppState::auto_bitrate_mbps(res.width, res.height, cur_fps);
    let min_cur_mbps = AppState::min_mbps_for_mode(res.width, res.height, cur_fps);
    let cur_fps_under_bw = !is_auto_bw && cur_mbps < min_cur_mbps;

    let mut hz_row = row()
        .w_full()
        .center_y()
        .gap(4.0)
        .child(text("Cadence (Hz)").font(k.ty.ui).color(k.c.text_base).grow(1.0));
    for (hz, key) in [
        (24_u32, "bench_hz_24"),
        (60_u32, "bench_hz_60"),
        (120_u32, "bench_hz_120"),
        (240_u32, "bench_hz_240"),
        (480_u32, "bench_hz_480"),
    ] {
        let enough_bw = is_auto_bw || cur_mbps >= AppState::min_mbps_for_mode(res.width, res.height, hz);
        let selected = cur_fps == hz && enough_bw;
        hz_row = hz_row.child(
            Button::new(hz.to_string())
                .variant(if selected {
                    ButtonVariant::Submit
                } else {
                    ButtonVariant::Outline
                })
                .control_size(ControlSize::Small)
                .disabled(!enough_bw)
                .on_press(move |s: &mut AppState| {
                    s.bench_fps = hz;
                    s.bench_fps_edit = TextEdit::new(hz.to_string());
                })
                .build(k)
                .key(key),
        );
    }
    hz_row = hz_row
        .child(
            TextInput::new(&s.bench_fps_edit)
                .placeholder("120")
                .width(Some(58.0))
                .on_edit(|s: &mut AppState| &mut s.bench_fps_edit)
                .build(k)
                .key("bench_fps_input"),
        )
        .child(text("Hz").font(k.ty.small).color(k.c.text_muted));

    let video_cfg = panel(
        k,
        "video_cfg",
        "Paramètres Vidéo & Transport",
        None,
        col()
            .gap(10.0)
            .child(
                row()
                    .w_full()
                    .center_y()
                    .gap(8.0)
                    .child(text("Résolution").font(k.ty.ui).color(k.c.text_base).grow(1.0))
                    .child(
                        SegmentedControl::new(
                            &["720p", "1080p", "1440p", "4K"],
                            Some(s.res_idx),
                        )
                        .fit()
                        .on_pick(|s: &mut AppState, idx, _| {
                            s.res_idx = idx.min(RESOLUTIONS.len() - 1);
                        })
                        .build(k)
                        .key("res_seg"),
                    ),
            )
            .child(
                row()
                    .w_full()
                    .center_y()
                    .gap(6.0)
                    .child(text("Bande passante").font(k.ty.ui).color(k.c.text_base).grow(1.0))
                    .child(
                        Button::new("Auto")
                            .variant(if is_auto_bw {
                                ButtonVariant::Submit
                            } else {
                                ButtonVariant::Outline
                            })
                            .control_size(ControlSize::Small)
                            .on_press(|s: &mut AppState| {
                                s.bench_bitrate_edit = TextEdit::new("");
                            })
                            .build(k)
                            .key("bench_bitrate_auto"),
                    )
                    .child(
                        SegmentedControl::new(
                            &["15", "25", "50", "100", "200"],
                            if is_auto_bw {
                                None
                            } else {
                                match cur_mbps {
                                    15 => Some(0),
                                    25 => Some(1),
                                    50 => Some(2),
                                    100 => Some(3),
                                    200 => Some(4),
                                    _ => None,
                                }
                            },
                        )
                        .fit()
                        .on_pick(|s: &mut AppState, idx, _| {
                            let mbps = match idx {
                                0 => 15,
                                1 => 25,
                                2 => 50,
                                3 => 100,
                                _ => 200,
                            };
                            s.bitrate_mbps = mbps;
                            s.bench_bitrate_edit = TextEdit::new(mbps.to_string());
                        })
                        .build(k)
                        .key("bench_bitrate_seg"),
                    )
                    .child(
                        TextInput::new(&s.bench_bitrate_edit)
                            .placeholder(format!("auto ({auto_mbps})"))
                            .width(Some(72.0))
                            .on_edit(|s: &mut AppState| &mut s.bench_bitrate_edit)
                            .build(k)
                            .key("bench_bitrate_input"),
                    )
                    .child(text("Mbps").font(k.ty.small).color(k.c.text_muted)),
            )
            .child(hz_row)
            .child(if cur_fps_under_bw {
                text(format!(
                    "Débit insuffisant pour {}p @ {} Hz (min. {} Mbps) — cliquez Auto ou augmentez les Mbps.",
                    res.height, cur_fps, min_cur_mbps
                ))
                .font(k.ty.small)
                .color(k.c.text_accent)
            } else {
                el().size(0.0, 0.0)
            })
            .child(stepper(
                k,
                "step_frames",
                "Frames mesurées",
                format!("{}", s.frames),
                |s| s.frames = s.frames.saturating_sub(60).max(60),
                |s| s.frames = (s.frames + 60).min(2400),
            ))
            .child(stepper(
                k,
                "step_fec",
                "Parité FEC Cauchy GF(2⁸)",
                if s.fec_shards == 0 {
                    "Off".to_owned()
                } else {
                    format!("{} shard(s)", s.fec_shards)
                },
                |s| s.fec_shards = s.fec_shards.saturating_sub(1),
                |s| s.fec_shards = (s.fec_shards + 1).min(4),
            ))
            .child(Divider::horizontal().build(k))
            .child(if host_supports_rio() {
                Switch::new(s.use_rio)
                    .label("Winsock RIO Batch TX")
                    .description("I/O enregistrée pour réduire les syscalls UDP (Zcvid / Pyrowave)")
                    .on_toggle(|s: &mut AppState| s.use_rio = !s.use_rio)
                    .build(k)
                    .key("sw_rio")
            } else {
                text("Transport UDP POSIX standard actif (Winsock RIO spécifique Windows).")
                    .font(k.ty.small)
                    .color(k.c.text_muted)
            })
            .child(
                Switch::new(s.live_window)
                    .label("Fenêtre vidéo live (--live)")
                    .description("Affiche le rendu décodé en temps réel pendant le benchmark")
                    .on_toggle(|s: &mut AppState| s.live_window = !s.live_window)
                    .build(k)
                    .key("sw_live"),
            )
            .child(
                Switch::new(s.measure_psnr)
                    .label("Mesure Y-PSNR VRAM (--psnr)")
                    .description("Vérifie la boucle fermée encodeur/décodeur (Zcvid)")
                    .on_toggle(|s: &mut AppState| s.measure_psnr = !s.measure_psnr)
                    .build(k)
                    .key("sw_psnr"),
            ),
    );

    let impair_cfg = panel(
        k,
        "impair_cfg",
        "Simulateur de Réseau Dégradé",
        Some("Injection de pertes, bursts Gilbert-Elliott, réordonnancement et gigue"),
        col()
            .gap(10.0)
            .child(
                row()
                    .w_full()
                    .gap(8.0)
                    .center_y()
                    .child(
                        Switch::new(s.impair.nasty_preset)
                            .label("Preset --nasty (5% loss, 8% burst, 15% reorder, 2-5 ms OWD)")
                            .on_toggle(|s: &mut AppState| {
                                if s.impair.nasty_preset {
                                    s.impair.clear();
                                } else {
                                    s.impair.apply_nasty();
                                }
                            })
                            .build(k)
                            .key("sw_nasty"),
                    )
                    .child(spacer())
                    .child(
                        Button::new("Réinitialiser")
                            .variant(ButtonVariant::Ghost)
                            .control_size(ControlSize::Small)
                            .on_press(|s: &mut AppState| s.impair.clear())
                            .build(k)
                            .key("btn_clear_impair"),
                    ),
            )
            .child(Divider::horizontal().build(k))
            .child(stepper(
                k,
                "step_loss",
                "Perte aléatoire (--loss)",
                format!("{} %", s.impair.loss_pct),
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.loss_pct = (s.impair.loss_pct - 1).max(0);
                },
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.loss_pct = (s.impair.loss_pct + 1).min(50);
                },
            ))
            .child(stepper(
                k,
                "step_burst",
                "Probabilité Burst (--burst)",
                format!("{} % (len {})", s.impair.burst_prob_pct, s.impair.burst_len),
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.burst_prob_pct = (s.impair.burst_prob_pct - 2).max(0);
                },
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.burst_prob_pct = (s.impair.burst_prob_pct + 2).min(50);
                },
            ))
            .child(stepper(
                k,
                "step_reorder",
                "Réordonnancement (--reorder)",
                format!("{} %", s.impair.reorder_pct),
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.reorder_pct = (s.impair.reorder_pct - 5).max(0);
                },
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.reorder_pct = (s.impair.reorder_pct + 5).min(80);
                },
            ))
            .child(stepper(
                k,
                "step_dup",
                "Duplication (--dup)",
                format!("{} %", s.impair.dup_pct),
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.dup_pct = (s.impair.dup_pct - 2).max(0);
                },
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.dup_pct = (s.impair.dup_pct + 2).min(50);
                },
            ))
            .child(stepper(
                k,
                "step_owd",
                "Délai OWD (--owd)",
                format!("{} µs", s.impair.base_owd_us),
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.base_owd_us = s.impair.base_owd_us.saturating_sub(500);
                },
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.base_owd_us = (s.impair.base_owd_us + 500).min(50_000);
                },
            ))
            .child(stepper(
                k,
                "step_jitter",
                "Gigue (--jitter)",
                format!("{} µs", s.impair.jitter_us),
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.jitter_us = s.impair.jitter_us.saturating_sub(500);
                },
                |s| {
                    s.impair.nasty_preset = false;
                    s.impair.jitter_us = (s.impair.jitter_us + 500).min(50_000);
                },
            )),
    );

    let action_bar = row()
        .key("action_bar")
        .w_full()
        .padding(14.0)
        .radius(k.radius.lg)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .center_y()
        .gap(12.0)
        .child(if s.bench_running {
            Button::new("Arrêter")
                .variant(ButtonVariant::Danger)
                .control_size(ControlSize::Large)
                .on_press(|s: &mut AppState| {
                    if let Some(stop) = &s.bench_stop {
                        stop.store(true, Ordering::Relaxed);
                    }
                })
                .build(k)
                .key("btn_stop_bench")
        } else {
            Button::new("Lancer le benchmark")
                .variant(ButtonVariant::Submit)
                .control_size(ControlSize::Large)
                .disabled(!pipe_supported)
                .on_press(|s: &mut AppState| {
                    if !s.pipeline.is_supported_on_platform() {
                        return;
                    }
                    s.bench_running = true;
                    s.bench_exit_code = None;
                    s.bench_logs.clear();
                    s.bench_results = BenchResults::default();
                    let stop = Arc::new(AtomicBool::new(false));
                    s.bench_stop = Some(stop.clone());
                    let args = s.build_bench_args();
                    spawn_benchmark(s.pipeline, args, s.tx.clone(), s.waker.clone(), stop);
                })
                .build(k)
                .key("btn_run_bench")
        })
        .child(if !pipe_supported {
            text("Pipeline NVENC réservé à Windows x64 — sélectionnez Zcvid Unified ou Pyrowave.")
                .font(k.ty.small)
                .color(k.c.text_accent)
        } else if s.bench_running {
            row()
                .gap(8.0)
                .center_y()
                .child(Spinner::new().size(16.0).color(k.c.bg_accent).build(k))
                .child(text("Exécution en cours...").font(k.ty.ui).color(k.c.text_muted))
        } else {
            el().size(0.0, 0.0)
        })
        .child(
            el().grow(1.0)
                .px(10.0)
                .py(6.0)
                .radius(k.radius.md)
                .fill(k.c.layer_02)
                .child(
                    text(s.cli_preview())
                        .font(k.ty.mono)
                        .color(k.c.text_muted),
                ),
        )
        .child(
            Button::new(if s.show_raw_logs { "Masquer logs" } else { "Logs bruts" })
                .variant(ButtonVariant::Outline)
                .control_size(ControlSize::Regular)
                .on_press(|s: &mut AppState| s.show_raw_logs = !s.show_raw_logs)
                .build(k)
                .key("btn_toggle_logs"),
        );

    let results_section = render_results(s, k);

    let mut root = col()
        .key("bench_view")
        .w_full()
        .gap(16.0)
        .child(pipeline_bar)
        .child(
            row()
                .w_full()
                .gap(16.0)
                .child(video_cfg.grow(1.0))
                .child(impair_cfg.grow(1.0)),
        )
        .child(action_bar)
        .child(results_section);

    if s.show_raw_logs || (!s.bench_logs.is_empty() && s.bench_results.rows.is_empty()) {
        let log_lines = if s.bench_logs.is_empty() {
            vec!["Aucune sortie pour le moment.".to_owned()]
        } else {
            s.bench_logs.clone()
        };
        let mut log_col = col().w_full().gap(2.0);
        for (i, line) in log_lines.iter().enumerate() {
            log_col = log_col.child(
                text(line.clone())
                    .key(format!("log_{i}"))
                    .font(k.ty.mono)
                    .color(k.c.text_base),
            );
        }
        root = root.child(panel(
            k,
            "raw_logs_panel",
            "Console Benchmark",
            None,
            col()
                .h(200.0)
                .w_full()
                .padding(10.0)
                .radius(k.radius.md)
                .fill(k.c.bg_base)
                .child(log_col.scroll(ScrollSpec { stick_bottom: true }).min_h(0.0)),
        ));
    }

    root
}

fn render_results(s: &AppState, k: &Kit) -> El {
    let r = &s.bench_results;
    if r.rows.is_empty() && r.frames_sent.is_none() {
        let msg = match s.bench_exit_code {
            Some(0) => "Benchmark terminé.",
            Some(code) => {
                if code == -2 {
                    "Benchmark interrompu par l'utilisateur."
                } else {
                    "Le benchmark s'est terminé avec une erreur (voir logs)."
                }
            }
            None => "Configure les paramètres ci-dessus et clique sur « Lancer le benchmark » pour mesurer la latence GPU + zcrudp.",
        };
        return panel(
            k,
            "results_empty",
            "Résultats & Latence",
            None,
            text(msg.to_owned()).font(k.ty.body).color(k.c.text_muted),
        );
    }

    let delivered_str = match (r.frames_delivered, r.frames_sent, r.delivered_pct) {
        (Some(d), Some(t), Some(p)) => format!("{d}/{t} ({p:.1}%)"),
        (Some(d), Some(t), None) => format!("{d}/{t}"),
        _ => "—".into(),
    };
    let bitrate_str = match (r.avg_mbps, r.kb_per_frame) {
        (Some(m), Some(kb)) => format!("{m:.2} Mbps ({kb:.1} KB/f)"),
        (Some(m), None) => format!("{m:.2} Mbps"),
        _ => "—".into(),
    };
    let fec_str = match (r.fec_recovered, r.frames_dropped) {
        (Some(f), Some(d)) => format!("{f} récup. / {d} perdues"),
        _ => "—".into(),
    };
    let psnr_or_pkts = if let Some(psnr) = r.y_psnr_db {
        match r.ref_mismatch_px {
            Some(px) => format!("{psnr:.2} dB (diff: {px} px)"),
            None => format!("{psnr:.2} dB"),
        }
    } else if let Some(pkts) = r.pkts_sent {
        format!("{pkts} pkts TX")
    } else {
        "—".into()
    };

    let kpis = row()
        .w_full()
        .gap(12.0)
        .child(kpi_box(k, "kpi_deliv", "Frames délivrées", delivered_str, Some(k.c.success.fg)))
        .child(kpi_box(k, "kpi_mbps", "Débit moyen", bitrate_str, None))
        .child(kpi_box(k, "kpi_fec", "FEC / Pertes", fec_str, None))
        .child(kpi_box(
            k,
            "kpi_psnr",
            if r.y_psnr_db.is_some() { "Y-PSNR (VRAM)" } else { "Paquets réseau" },
            psnr_or_pkts,
            Some(k.c.text_accent),
        ));

    let max_p50 = r
        .rows
        .iter()
        .map(|row| row.p50_us)
        .fold(1.0_f64, f64::max);

    let mut table = col().w_full().gap(6.0).child(
        row()
            .w_full()
            .px(8.0)
            .py(4.0)
            .child(text("Étape").font(k.ty.small_medium).color(k.c.text_muted).w(180.0))
            .child(text("Barre p50").font(k.ty.small_medium).color(k.c.text_muted).grow(1.0))
            .child(text("min").font(k.ty.small_medium).color(k.c.text_muted).w(80.0))
            .child(text("p50").font(k.ty.small_medium).color(k.c.text_muted).w(80.0))
            .child(text("p95").font(k.ty.small_medium).color(k.c.text_muted).w(80.0))
            .child(text("p99").font(k.ty.small_medium).color(k.c.text_muted).w(80.0))
            .child(text("avg").font(k.ty.small_medium).color(k.c.text_muted).w(80.0)),
    );

    for (idx, row_stat) in r.rows.iter().enumerate() {
        let is_total = row_stat.label.contains("TOTAL");
        let frac = ((row_stat.p50_us / max_p50) as f32).clamp(0.02, 1.0);
        let bar_color = if is_total { k.c.bg_accent } else { k.c.success.fg };
        let txt_color = if is_total { k.c.text_strong } else { k.c.text_base };

        table = table.child(
            row()
                .key(format!("stat_row_{idx}"))
                .w_full()
                .px(8.0)
                .py(6.0)
                .radius(k.radius.sm)
                .fill(if is_total { k.c.layer_03 } else { k.c.layer_02 })
                .center_y()
                .gap(8.0)
                .child(text(row_stat.label.clone()).font(k.ty.ui).color(txt_color).w(180.0))
                .child(
                    row()
                        .grow(1.0)
                        .h(8.0)
                        .radius(4.0)
                        .fill(k.c.bg_base)
                        .child(
                            el().grow(frac)
                                .h(8.0)
                                .radius(4.0)
                                .fill(bar_color),
                        )
                        .child(el().grow((1.0 - frac).max(0.0)).h(8.0)),
                )
                .child(text(format!("{:.1} µs", row_stat.min_us)).font(k.ty.mono).color(k.c.text_muted).w(80.0))
                .child(text(format!("{:.1} µs", row_stat.p50_us)).font(k.ty.mono).color(txt_color).w(80.0))
                .child(text(format!("{:.1} µs", row_stat.p95_us)).font(k.ty.mono).color(k.c.text_muted).w(80.0))
                .child(text(format!("{:.1} µs", row_stat.p99_us)).font(k.ty.mono).color(k.c.text_muted).w(80.0))
                .child(text(format!("{:.1} µs", row_stat.avg_us)).font(k.ty.mono).color(k.c.text_muted).w(80.0)),
        );
    }

    let subtitle = if !r.adapter.is_empty() {
        Some(r.adapter.as_str())
    } else {
        None
    };

    panel(
        k,
        "results_panel",
        "Résultats & Latence",
        subtitle,
        col().w_full().gap(14.0).child(kpis).child(table),
    )
}
