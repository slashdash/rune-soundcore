use rune::tok::{self, motion, space, type_};
use rune::{Color, Drag, El, Phase, el, row, text};
use rune_kit::Kit;
use rune_kit::Switch;

use crate::state::AppState;

/// Everything a control needs to paint itself.
#[derive(Clone)]
pub struct Paint {
    pub t: tok::Theme,
    pub kit: Kit,
    pub accent: Color,
    pub hair: f32,
}

pub fn section(p: &Paint, key: impl Into<String>, title: &str, rows: Vec<El>) -> El {
    let count = rows.len();
    let mut card = super::card(p);
    for (i, row) in rows.into_iter().enumerate() {
        card = card.child(row);
        if i + 1 < count {
            card = card.child(
                el().key(format!("sep{i}"))
                    .h(p.hair)
                    .ml(space::S3)
                    .mr(space::S3)
                    .fill(p.t.separator)
                    .a11y_hidden(),
            );
        }
    }
    rune::col()
        .key(key.into())
        .gap(space::S2)
        .child(
            text(title)
                .key("header")
                .font(type_::SECTION)
                .color(p.t.text_secondary)
                .ml(space::S3),
        )
        .child(card)
}

pub fn label_row(p: &Paint, key: String, label: String, control: El) -> El {
    row()
        .key(key)
        .h(44.0)
        .px(space::S3)
        .center_y()
        .justify_between()
        .gap(space::S3)
        .label(label.clone())
        .child(
            text(label)
                .key("label")
                .font(type_::BODY)
                .color(p.t.text)
                .a11y_hidden(),
        )
        .child(control)
}

pub fn toggle_row(
    p: &Paint,
    key: String,
    label: String,
    checked: bool,
    on_toggle: impl Fn(&mut AppState) + 'static,
) -> El {
    let switch = Switch::new(checked)
        .hide_label()
        .disabled(false)
        .on_toggle(on_toggle)
        .build(&p.kit)
        .a11y_hidden();
    label_row(p, key, label, switch)
        .role("switch")
        .checked(checked)
}

/// A slider with its value on the right; drags report their steps to `on_change` and the
/// caller decides when to write to the device.
#[allow(clippy::too_many_arguments)]
pub fn slider_row(
    p: &Paint,
    key: String,
    label: String,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    display: String,
    on_change: impl Fn(&mut AppState, Phase, f32) + Clone + 'static,
) -> El {
    let (w, h, k, bar) = (190.0, 22.0, 14.0, 4.0);
    let travel = w - k;
    let frac = if max > min {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let to_value = move |x: f32| {
        let raw = min + ((x - k / 2.0) / travel).clamp(0.0, 1.0) * (max - min);
        let stepped = ((raw - min) / step).round() * step + min;
        stepped.clamp(min, max)
    };

    let drag_handler = {
        let on_change = on_change.clone();
        move |s: &mut AppState, d: &mut Drag, _tx: &mut rune::Tx| match d.phase {
            Phase::Down => {
                d.data = value;
                if (d.x - (k / 2.0 + frac * travel)).abs() > k / 2.0 + 2.0 {
                    on_change(s, Phase::Move, to_value(d.x));
                }
            }
            Phase::Move => on_change(s, Phase::Move, to_value(d.x)),
            Phase::Up => {
                if d.cancelled() {
                    on_change(s, Phase::Up, d.data);
                } else {
                    on_change(s, Phase::Up, to_value(d.x));
                }
            }
        }
    };
    let step_handler = {
        let on_change = on_change.clone();
        move |s: &mut AppState, steps: i32, _tx: &mut rune::Tx| {
            let next = (value + steps as f32 * step).clamp(min, max);
            on_change(s, Phase::Up, next);
        }
    };

    let slider = el()
        .key(key.clone())
        .role("slider")
        .label(label.clone())
        .range_value(f64::from(value), f64::from(min), f64::from(max))
        .range_step(f64::from(step))
        .a11y_value(display.clone())
        .size(w, h)
        .motion(motion::SNAPPY)
        .on_step(step_handler)
        .on_drag(drag_handler)
        .child(
            el().key("track")
                .size(w, bar)
                .absolute(0.0, (h - bar) / 2.0)
                .radius(bar / 2.0)
                .fill(p.t.control_off)
                .a11y_hidden(),
        )
        .child(
            el().key("fill")
                .size(k / 2.0 + frac * travel, bar)
                .absolute(0.0, (h - bar) / 2.0)
                .radius(bar / 2.0)
                .fill(p.accent)
                .a11y_hidden(),
        )
        .child(
            el().key("knob")
                .size(k, k)
                .absolute(frac * travel, (h - k) / 2.0)
                .radius(k / 2.0)
                .fill(p.t.knob)
                .shadow(1.0, 3.0, p.t.knob_shadow)
                .shadow(0.0, 0.5, p.t.edge)
                .a11y_hidden(),
        );

    row()
        .key(format!("{key}-row"))
        .h(44.0)
        .px(space::S3)
        .center_y()
        .justify_between()
        .gap(space::S3)
        .label(label.clone())
        .child(
            text(label)
                .key("label")
                .font(type_::BODY)
                .color(p.t.text)
                .a11y_hidden(),
        )
        .child(
            row().gap(space::S3).center_y().child(slider).child(
                text(display)
                    .key("value")
                    .w(52.0)
                    .font(type_::MONO_CAPTION)
                    .color(p.t.text_secondary)
                    .align(rune::Align::End)
                    .a11y_hidden(),
            ),
        )
}

pub fn info_row(p: &Paint, key: String, label: String, value: String) -> El {
    label_row(
        p,
        key,
        label,
        text(value)
            .key("value")
            .font(type_::MONO_CAPTION)
            .color(p.t.text_secondary),
    )
}
