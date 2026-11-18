use rune::{El, ScrollSpec, col, row, spacer, text};
use rune_kit::{Badge, BadgeVariant, Button, ButtonVariant, ControlSize, Kit, Spinner};

use crate::runner::spawn_test;
use crate::state::{AppState, TestStatus};
use crate::views::panel;

pub fn view(s: &AppState, k: &Kit) -> El {
    let any_running = s.tests.iter().any(|t| t.status == TestStatus::Running);

    let top_bar = row()
        .w_full()
        .center_y()
        .gap(12.0)
        .child(
            text("Suites de tests RUDP (zcstream, zcvid, zcrudp, zpsync)")
                .font(k.ty.body_medium)
                .color(k.c.text_strong),
        )
        .child(spacer())
        .child(
            Button::new(if any_running { "Tests en cours..." } else { "Tout lancer" })
                .variant(ButtonVariant::Submit)
                .control_size(ControlSize::Large)
                .disabled(any_running)
                .on_press(|s: &mut AppState| {
                    for entry in &mut s.tests {
                        entry.status = TestStatus::Running;
                        entry.duration_ms = None;
                        entry.logs.clear();
                        spawn_test(entry.id, s.tx.clone(), s.waker.clone());
                    }
                })
                .build(k)
                .key("btn_run_all_tests"),
        );

    let mut list = col().w_full().gap(10.0);
    for (idx, entry) in s.tests.iter().enumerate() {
        let is_selected = s.selected_test == idx;
        let is_running = entry.status == TestStatus::Running;
        let badge_variant = match entry.status {
            TestStatus::Idle => BadgeVariant::Neutral,
            TestStatus::Running => BadgeVariant::Subtle,
            TestStatus::Passed => BadgeVariant::Accent,
            TestStatus::Failed => BadgeVariant::Neutral,
        };
        let dur = entry
            .duration_ms
            .map(|ms| format!("{ms} ms"))
            .unwrap_or_else(|| "—".to_owned());
        let id = entry.id;

        list = list.child(
            row()
                .key(format!("test_row_{idx}"))
                .w_full()
                .padding(12.0)
                .radius(k.radius.md)
                .fill(if is_selected { k.c.layer_03 } else { k.c.layer_02 })
                .border(
                    1.0,
                    if is_selected { k.c.border_strong } else { k.c.border_weak },
                )
                .center_y()
                .gap(12.0)
                .on_press(move |s: &mut AppState| s.selected_test = idx)
                .child(
                    col()
                        .grow(1.0)
                        .gap(2.0)
                        .child(text(entry.name.to_owned()).font(k.ty.body_medium).color(k.c.text_strong))
                        .child(text(entry.subtitle.to_owned()).font(k.ty.small).color(k.c.text_muted)),
                )
                .child(if is_running {
                    Spinner::new().size(14.0).color(k.c.bg_accent).build(k)
                } else {
                    Badge::new(entry.status.label())
                        .variant(badge_variant)
                        .build(k)
                })
                .child(text(dur).font(k.ty.mono).color(k.c.text_muted).w(72.0))
                .child(
                    Button::new("Exécuter")
                        .variant(ButtonVariant::Outline)
                        .control_size(ControlSize::Regular)
                        .disabled(is_running)
                        .on_press(move |s: &mut AppState| {
                            s.selected_test = idx;
                            if let Some(t) = s.tests.get_mut(idx) {
                                t.status = TestStatus::Running;
                                t.duration_ms = None;
                                t.logs.clear();
                            }
                            spawn_test(id, s.tx.clone(), s.waker.clone());
                        })
                        .build(k)
                        .key(format!("btn_run_test_{idx}")),
                ),
        );
    }

    let selected = &s.tests[s.selected_test.min(s.tests.len() - 1)];
    let mut log_col = col().w_full().gap(2.0);
    if selected.logs.is_empty() {
        log_col = log_col.child(
            text("Clique sur « Exécuter » ou « Tout lancer » pour voir la sortie des tests.")
                .font(k.ty.mono)
                .color(k.c.text_muted),
        );
    } else {
        for (i, line) in selected.logs.iter().enumerate() {
            log_col = log_col.child(
                text(line.clone())
                    .key(format!("tlog_{i}"))
                    .font(k.ty.mono)
                    .color(k.c.text_base),
            );
        }
    }

    col()
        .key("tests_view")
        .w_full()
        .gap(16.0)
        .child(panel(k, "tests_list_panel", "Console de Tests", None, col().gap(12.0).child(top_bar).child(list)))
        .child(panel(
            k,
            "tests_log_panel",
            &format!("Logs — {}", selected.name),
            None,
            col()
                .h(280.0)
                .w_full()
                .padding(10.0)
                .radius(k.radius.md)
                .fill(k.c.bg_base)
                .child(log_col.scroll(ScrollSpec { stick_bottom: true }).min_h(0.0)),
        ))
}
