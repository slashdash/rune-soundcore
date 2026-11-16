use openscq30_i18n::Translate;
use openscq30_lib::DeviceModel;
use openscq30_lib::settings::{
    CategoryId, MultiSelectWithRemoveCommand, Setting, SettingId, Value,
};
use rune::tok::{space, type_};
use rune::{El, Phase, col, el, row, text};
use rune_kit::{Button, ButtonVariant, Checkbox, ControlSize, Select};
use strum::VariantArray;

use super::controls::{Paint, info_row, section, slider_row, toggle_row};
use crate::bt::Cmd;
use crate::state::{AppState, Connection};

const EMPTY: &str = "No device connected";

pub fn devices(state: &AppState, p: &Paint, layer: &mut Vec<El>) -> El {
    let mut sections: Vec<El> = Vec::new();

    let models: Vec<String> = DeviceModel::VARIANTS
        .iter()
        .map(|m| m.translate())
        .collect();
    let key = "model-select".to_owned();
    let selected = state.model.translate();
    let options = DeviceModel::VARIANTS.to_vec();
    let labels = models.clone();
    let model_select = Select::new(key.clone(), &models)
        .current(Some(selected.as_str()))
        .open(state.open_select.as_deref() == Some(key.as_str()))
        .on_toggle({
            let key = key.clone();
            move |s: &mut AppState| {
                s.open_select = if s.open_select.as_deref() == Some(key.as_str()) {
                    None
                } else {
                    Some(key.clone())
                };
            }
        })
        .on_close(|s: &mut AppState| s.open_select = None)
        .on_choose(move |s: &mut AppState, chosen: &str, _| {
            s.open_select = None;
            if let Some(model) = labels
                .iter()
                .position(|label| label == chosen)
                .and_then(|index| options.get(index))
            {
                s.model = *model;
            }
        })
        .build(&p.kit, layer);

    sections.push(section(
        p,
        "model",
        "Model",
        vec![
            row()
                .key("model-row")
                .h(52.0)
                .px(space::S3)
                .center_y()
                .justify_between()
                .gap(space::S3)
                .label("Model")
                .child(
                    text("Model")
                        .key("label")
                        .font(type_::BODY)
                        .color(p.t.text)
                        .a11y_hidden(),
                )
                .child(model_select),
        ],
    ));

    sections.push(section(
        p,
        "scan",
        "Bluetooth",
        vec![
            row()
                .key("scan-row")
                .h(52.0)
                .px(space::S3)
                .center_y()
                .justify_between()
                .gap(space::S3)
                .child(
                    text(if state.scanning {
                        "Scanning…"
                    } else {
                        "Look for paired earbuds"
                    })
                    .key("label")
                    .font(type_::BODY)
                    .color(p.t.text_secondary),
                )
                .child(
                    Button::new("Scan")
                        .variant(ButtonVariant::Contrast)
                        .control_size(ControlSize::Regular)
                        .disabled(state.scanning)
                        .on_press(|s: &mut AppState| s.send(Cmd::Scan(s.model)))
                        .build(&p.kit),
                ),
        ],
    ));

    if !state.paired.is_empty() {
        let rows = state
            .paired
            .iter()
            .map(|paired| {
                let mac = paired.mac_address;
                row()
                    .key(format!("paired-{mac}"))
                    .h(52.0)
                    .px(space::S3)
                    .center_y()
                    .justify_between()
                    .gap(space::S3)
                    .child(
                        col()
                            .key("text")
                            .gap(2.0)
                            .child(
                                text(paired.model.translate())
                                    .key("name")
                                    .font(type_::BODY)
                                    .color(p.t.text),
                            )
                            .child(
                                text(mac.to_string())
                                    .key("mac")
                                    .font(type_::MONO_CAPTION)
                                    .color(p.t.text_secondary),
                            ),
                    )
                    .child(
                        row()
                            .key("actions")
                            .gap(space::S2)
                            .child(
                                Button::new("Connect")
                                    .control_size(ControlSize::Small)
                                    .disabled(state.connection == Connection::Connecting)
                                    .on_press(move |s: &mut AppState| s.send(Cmd::Connect(mac)))
                                    .build(&p.kit),
                            )
                            .child(
                                Button::new("Forget")
                                    .variant(ButtonVariant::Ghost)
                                    .control_size(ControlSize::Small)
                                    .on_press(move |s: &mut AppState| s.send(Cmd::Forget(mac)))
                                    .build(&p.kit),
                            ),
                    )
            })
            .collect();
        sections.push(section(p, "paired", "Saved devices", rows));
    }

    if !state.devices.is_empty() {
        let rows = state
            .devices
            .iter()
            .map(|device| {
                let mac = device.mac_address;
                let model = state.model;
                row()
                    .key(format!("found-{mac}"))
                    .h(52.0)
                    .px(space::S3)
                    .center_y()
                    .justify_between()
                    .gap(space::S3)
                    .child(
                        col()
                            .key("text")
                            .gap(2.0)
                            .child(
                                text(device.name.clone())
                                    .key("name")
                                    .font(type_::BODY)
                                    .color(p.t.text),
                            )
                            .child(
                                text(mac.to_string())
                                    .key("mac")
                                    .font(type_::MONO_CAPTION)
                                    .color(p.t.text_secondary),
                            ),
                    )
                    .child(
                        Button::new("Use")
                            .control_size(ControlSize::Small)
                            .on_press(move |s: &mut AppState| {
                                s.send(Cmd::PairAndConnect(model, mac))
                            })
                            .build(&p.kit),
                    )
            })
            .collect();
        sections.push(section(p, "found", "Available devices", rows));
    }

    page_shell(
        p,
        "Devices",
        Some("Pair in System Settings first, then pick the earbuds here."),
        state.note.as_deref(),
        col().key("devices").gap(space::S6).children(sections),
    )
}

pub fn category(state: &AppState, p: &Paint, category: CategoryId, layer: &mut Vec<El>) -> El {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return page_shell(
            p,
            &category.translate(),
            Some(EMPTY),
            state.note.as_deref(),
            col().key("empty"),
        );
    };
    let Some(settings) = snapshot.category(&category) else {
        return page_shell(
            p,
            &category.translate(),
            Some(EMPTY),
            state.note.as_deref(),
            col().key("empty"),
        );
    };

    let mut rows: Vec<El> = Vec::new();
    for (id, setting) in settings {
        rows.extend(setting_rows(state, p, *id, setting, layer));
    }

    page_shell(
        p,
        &category.translate(),
        None,
        state.note.as_deref(),
        col()
            .key("category")
            .gap(space::S6)
            .child(section(p, "settings", "", rows)),
    )
}

fn page_shell(p: &Paint, title: &str, subtitle: Option<&str>, note: Option<&str>, body: El) -> El {
    let mut header = col()
        .key("header")
        .px(space::S6)
        .pt(space::S6)
        .pb(space::S3)
        .gap(2.0)
        .child(
            text(title.to_owned())
                .key("title")
                .font(type_::TITLE)
                .color(p.t.text),
        );
    if let Some(subtitle) = subtitle {
        header = header.child(
            text(subtitle.to_owned())
                .key("subtitle")
                .font(type_::BODY)
                .color(p.t.text_secondary),
        );
    }
    if let Some(note) = note {
        header = header.child(
            text(note.to_owned())
                .key("note")
                .font(type_::LABEL)
                .color(p.t.text_secondary),
        );
    }

    col()
        .key("main")
        .grow(1.0)
        .h_full()
        .min_h(0.0)
        .child(header)
        .child(
            el().key("scroll")
                .grow(1.0)
                .min_h(0.0)
                .scroll(rune::ScrollSpec {
                    stick_bottom: false,
                })
                .child(
                    col()
                        .key("content")
                        .w_full()
                        .min_h(0.0)
                        .px(space::S6)
                        .pb(space::S6)
                        .child(body),
                ),
        )
}

fn setting_rows(
    state: &AppState,
    p: &Paint,
    id: SettingId,
    setting: &Setting,
    layer: &mut Vec<El>,
) -> Vec<El> {
    let label = id.translate();
    let key = format!("setting-{id:?}");

    match setting {
        Setting::Toggle { value } => {
            let value = *value;
            vec![toggle_row(p, key, label, value, move |s: &mut AppState| {
                s.send(Cmd::Set(id, Value::Bool(!value)))
            })]
        }
        Setting::I32Range { setting, value } => {
            let min = *setting.range.start() as f32;
            let max = *setting.range.end() as f32;
            let step = (setting.step.max(1)) as f32;
            let current = state.slider_value(&id, *value as f32);
            vec![slider_row(
                p,
                key,
                label,
                current,
                min,
                max,
                step,
                format!("{}", current.round() as i32),
                move |s: &mut AppState, phase: Phase, value: f32| {
                    s.draft.insert(id, value);
                    if phase == Phase::Up {
                        s.send(Cmd::Set(id, Value::I32(value.round() as i32)));
                    }
                },
            )]
        }
        Setting::Select { setting, value } => vec![select_row(
            state,
            p,
            id,
            label,
            setting,
            Some(value.as_ref()),
            false,
            layer,
        )],
        Setting::OptionalSelect { setting, value } => vec![select_row(
            state,
            p,
            id,
            label,
            setting,
            value.as_ref().map(|value| value.as_ref()),
            true,
            layer,
        )],
        Setting::PresetEqualizerProfileSelect { select, value, .. } => vec![select_row(
            state,
            p,
            id,
            label,
            select,
            value.as_ref().map(|value| value.as_ref()),
            true,
            layer,
        )],
        Setting::ModifiableSelect { setting, value } => {
            let mut rows = vec![select_row(
                state,
                p,
                id,
                label,
                setting,
                value.as_ref().map(|value| value.as_ref()),
                true,
                layer,
            )];
            if let Some(current) = value.clone() {
                rows.push(
                    row()
                        .key(format!("{key}-delete"))
                        .h(44.0)
                        .px(space::S3)
                        .center_y()
                        .justify_between()
                        .child(
                            text("Delete this profile")
                                .key("label")
                                .font(type_::BODY)
                                .color(p.t.text_secondary),
                        )
                        .child(
                            Button::new("Delete")
                                .variant(ButtonVariant::Ghost)
                                .control_size(ControlSize::Small)
                                .on_press(move |s: &mut AppState| {
                                    s.send(Cmd::Set(
                                    id,
                                    Value::ModifiableSelectCommand(
                                        openscq30_lib::settings::ModifiableSelectCommand::Remove(
                                            current.clone(),
                                        ),
                                    ),
                                ))
                                })
                                .build(&p.kit),
                        ),
                );
            }
            rows
        }
        Setting::MultiSelect { setting, values } => {
            let mut rows = Vec::new();
            for (option, localized) in setting.options.iter().zip(setting.localized_options.iter())
            {
                let checked = values.contains(option);
                let option = option.clone();
                let current = values.clone();
                let toggle = {
                    let option = option.clone();
                    move |s: &mut AppState| {
                        let mut next = current.clone();
                        if next.contains(&option) {
                            next.retain(|value| value != &option);
                        } else {
                            next.push(option.clone());
                        }
                        s.send(Cmd::Set(id, Value::StringVec(next)));
                    }
                };
                rows.push(
                    row()
                        .key(format!("{key}-{localized}"))
                        .h(40.0)
                        .px(space::S3)
                        .center_y()
                        .label(localized.clone())
                        .child(
                            Checkbox::new(checked)
                                .label(localized.clone())
                                .on_toggle(toggle)
                                .build(&p.kit),
                        ),
                );
            }
            rows
        }
        Setting::MultiSelectWithRemove { values, .. } => values
            .iter()
            .map(|value| {
                let value = value.clone();
                row()
                    .key(format!("{key}-{value}"))
                    .h(44.0)
                    .px(space::S3)
                    .center_y()
                    .justify_between()
                    .child(
                        text(value.to_string())
                            .key("label")
                            .font(type_::BODY)
                            .color(p.t.text),
                    )
                    .child(
                        Button::new("Remove")
                            .variant(ButtonVariant::Ghost)
                            .control_size(ControlSize::Small)
                            .on_press(move |s: &mut AppState| {
                                s.send(Cmd::Set(
                                    id,
                                    Value::MultiSelectWithRemoveCommand(
                                        MultiSelectWithRemoveCommand::Remove(value.clone()),
                                    ),
                                ))
                            })
                            .build(&p.kit),
                    )
            })
            .collect(),
        Setting::Equalizer { setting, value, .. } => {
            let bands = setting.band_hz.len().max(value.len());
            let mut rows = Vec::new();
            for band in 0..bands {
                let hz = setting.band_hz.get(band).copied().unwrap_or(0);
                let current = state
                    .eq_draft
                    .as_ref()
                    .and_then(|draft| draft.get(band).copied())
                    .or_else(|| value.get(band).copied())
                    .unwrap_or(0);
                let base = value.clone();
                rows.push(slider_row(
                    p,
                    format!("{key}-{hz}"),
                    format!("{hz} Hz"),
                    f32::from(current),
                    f32::from(setting.min),
                    f32::from(setting.max),
                    1.0,
                    format_db(i32::from(current), setting.fraction_digits),
                    move |s: &mut AppState, phase: Phase, level: f32| {
                        let mut values = s.eq_draft.clone().unwrap_or_else(|| base.clone());
                        if values.len() <= band {
                            values.resize(band + 1, 0);
                        }
                        values[band] = level.round() as i16;
                        s.eq_draft = Some(values.clone());
                        if phase == Phase::Up {
                            s.send(Cmd::Set(id, Value::I16Vec(values)));
                        }
                    },
                ));
            }
            rows
        }
        Setting::Information {
            translated_value, ..
        } => {
            vec![info_row(p, key, label, translated_value.clone())]
        }
        Setting::Action => vec![
            row()
                .key(key)
                .h(52.0)
                .px(space::S3)
                .center_y()
                .justify_between()
                .child(text(label).key("label").font(type_::BODY).color(p.t.text))
                .child(
                    Button::new("Execute")
                        .variant(ButtonVariant::Outline)
                        .control_size(ControlSize::Small)
                        .on_press(move |s: &mut AppState| s.send(Cmd::Set(id, Value::Bool(true))))
                        .build(&p.kit),
                ),
        ],
        _ => vec![info_row(
            p,
            key,
            label,
            "Not supported in this app yet".to_owned(),
        )],
    }
}

#[allow(clippy::too_many_arguments)]
fn select_row(
    state: &AppState,
    p: &Paint,
    id: SettingId,
    label: String,
    setting: &openscq30_lib::settings::Select,
    value: Option<&str>,
    optional: bool,
    layer: &mut Vec<El>,
) -> El {
    let key = format!("select-{id:?}");
    let none = "None".to_owned();
    let mut labels: Vec<String> = Vec::new();
    if optional {
        labels.push(none.clone());
    }
    labels.extend(setting.localized_options.iter().cloned());

    let index = value.and_then(|current| {
        setting
            .options
            .iter()
            .position(|option| option.as_ref() == current)
    });
    let current_label = index.map(|index| {
        let index = if optional { index + 1 } else { index };
        labels.get(index).cloned().unwrap_or_else(|| none.clone())
    });

    let options = setting.options.clone();
    let choose_labels = labels.clone();
    let choose_none = none.clone();
    let choose_key = key.clone();
    let select = Select::new(key.clone(), &labels)
        .current(current_label.as_deref())
        .placeholder("None")
        .open(state.open_select.as_deref() == Some(key.as_str()))
        .on_toggle({
            let key = key.clone();
            move |s: &mut AppState| {
                s.open_select = if s.open_select.as_deref() == Some(key.as_str()) {
                    None
                } else {
                    Some(key.clone())
                };
            }
        })
        .on_close(|s: &mut AppState| s.open_select = None)
        .on_choose(move |s: &mut AppState, chosen: &str, _| {
            s.open_select = None;
            let _ = &choose_key;
            let canonical = if optional && chosen == choose_none {
                None
            } else {
                choose_labels
                    .iter()
                    .position(|option| option == chosen)
                    .and_then(|index| {
                        let index = if optional {
                            index.saturating_sub(1)
                        } else {
                            index
                        };
                        options.get(index).cloned()
                    })
            };
            let value = if optional {
                Value::OptionalString(canonical)
            } else {
                match canonical {
                    Some(canonical) => Value::String(canonical),
                    None => return,
                }
            };
            s.send(Cmd::Set(id, value));
        })
        .width(220.0)
        .build(&p.kit, layer);

    row()
        .key(format!("{key}-row"))
        .h(52.0)
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
        .child(select)
}

fn format_db(value: i32, fraction_digits: i16) -> String {
    // Device-supplied: bound it so `pow` can't overflow.
    let digits = fraction_digits.clamp(0, 3) as u32;
    let divisor = 10i32.pow(digits);
    let sign = if value < 0 { "-" } else { "" };
    let whole = (value / divisor).abs();
    let fraction = (value % divisor).abs();
    if digits > 0 {
        format!("{sign}{whole}.{fraction:0width$}", width = digits as usize)
    } else {
        format!("{sign}{whole}")
    }
}

#[cfg(test)]
mod tests {
    use super::format_db;

    #[test]
    fn formats_db_values() {
        assert_eq!(format_db(0, 1), "0.0");
        assert_eq!(format_db(615, 1), "61.5");
        assert_eq!(format_db(-615, 1), "-61.5");
        assert_eq!(format_db(5, 2), "0.05");
        assert_eq!(format_db(-5, 2), "-0.05");
        assert_eq!(format_db(7, 0), "7");
    }

    #[test]
    fn clamps_hostile_fraction_digits() {
        assert_eq!(format_db(1234, 12), "1.234");
        assert_eq!(format_db(1234, -3), "1234");
    }
}
