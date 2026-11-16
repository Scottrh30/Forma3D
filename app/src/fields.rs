//! Small input widgets: number fields (type, or drag the label to scrub),
//! switches and drop-down choices.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

fn fmt(v: f32, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if s == "-0" || s.starts_with("-0.") && s.trim_start_matches("-0.").chars().all(|c| c == '0') { s.trim_start_matches('-').to_owned() } else { s }
}

/// A number with an optional coloured label (drag the label to change it).
#[component]
pub fn NumField(
    #[prop(into)] value: Signal<f32>,
    #[prop(optional)] label: &'static str,
    #[prop(optional)] color: &'static str,
    #[prop(optional)] unit: &'static str,
    #[prop(default = 2)] decimals: usize,
    #[prop(default = 1.0)] step: f32,
    on_change: Callback<f32>,
) -> impl IntoView {
    let scrub = RwSignal::new(None::<f32>);
    let start = StoredValue::new((0.0f64, 0.0f32));
    let shown = move || fmt(scrub.get().unwrap_or_else(|| value.get()), decimals);
    let commit = move |text: String| {
        let t: String = text.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')).collect();
        if let Ok(v) = t.parse::<f32>()
            && v.is_finite()
        {
            on_change.run(v);
        }
    };
    view! {
        <div class="num">
            {(!label.is_empty()).then(|| view! {
                <b
                    style=format!("color:{}", if color.is_empty() { "var(--text)" } else { color })
                    title="Drag to change"
                    on:pointerdown=move |ev: web_sys::PointerEvent| {
                        if let Some(t) = ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                            let _ = t.set_pointer_capture(ev.pointer_id());
                        }
                        start.set_value((ev.client_x() as f64, value.get_untracked()));
                        scrub.set(Some(value.get_untracked()));
                    }
                    on:pointermove=move |ev: web_sys::PointerEvent| {
                        if scrub.get_untracked().is_some() {
                            let (x0, v0) = start.get_value();
                            let fine = if ev.shift_key() { 0.1 } else { 1.0 };
                            scrub.set(Some(v0 + ((ev.client_x() as f64 - x0) as f32) * step * fine));
                        }
                    }
                    on:pointerup=move |_| {
                        if let Some(v) = scrub.get_untracked() {
                            scrub.set(None);
                            if v != value.get_untracked() {
                                on_change.run(v);
                            }
                        }
                    }
                >
                    {label}
                </b>
            })}
            <input
                prop:value=shown
                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    ev.stop_propagation();
                    if ev.key() == "Enter" {
                        commit(event_target_value(&ev));
                        if let Some(t) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok()) {
                            let _ = t.blur();
                        }
                    }
                }
                on:change=move |ev| commit(event_target_value(&ev))
                on:focus=move |ev| {
                    if let Some(t) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                        t.select();
                    }
                }
            />
            {(!unit.is_empty()).then(|| view! { <span class="unit">{unit}</span> })}
        </div>
    }
}

#[component]
pub fn Toggle(#[prop(into)] on: Signal<bool>, on_change: Callback<bool>) -> impl IntoView {
    view! { <button class="toggle" class:on=move || on.get() on:click=move |_| on_change.run(!on.get_untracked())></button> }
}

#[component]
pub fn Choice(options: Vec<(String, String)>, #[prop(into)] value: Signal<String>, on_change: Callback<String>) -> impl IntoView {
    view! {
        <select class="sel" on:change=move |ev| on_change.run(event_target_value(&ev))>
            {options
                .into_iter()
                .map(|(v, l)| {
                    let v2 = v.clone();
                    view! { <option value=v selected=move || value.get() == v2>{l}</option> }
                })
                .collect_view()}
        </select>
    }
}

pub fn rgb_hex(c: [f32; 3]) -> String {
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

pub fn parse_hex(s: &str) -> Option<[f32; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok().map(|v| v as f32 / 255.0);
    Some([p(0)?, p(2)?, p(4)?])
}
