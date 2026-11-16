//! The timeline under the view: frame ruler, playhead and transport.

use crate::icons::icon;
use crate::state::app;
use leptos::prelude::*;

#[component]
pub fn Timeline() -> impl IntoView {
    let app = app();
    let ruler = NodeRef::<leptos::html::Div>::new();
    let scrubbing = StoredValue::new(false);
    let range = move || app.read(|e| (e.timeline.min, e.timeline.max, e.timeline.frame, e.timeline.fps, e.timeline.playing));

    let frame_at = move |x: f64| {
        let Some(r) = ruler.get_untracked() else { return };
        let rect = r.get_bounding_client_rect();
        let t = ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
        app.view(|e| {
            let (lo, hi) = (e.timeline.min, e.timeline.max);
            e.timeline.frame = lo + ((hi - lo) as f64 * t).round() as i32;
        });
        app.rev.update(|r| *r += 1);
    };

    // Playback: advance with the display's refresh while playing.
    let last_t = StoredValue::new(0.0f64);
    let tick = std::rc::Rc::new(std::cell::RefCell::new(None::<Box<dyn Fn()>>));
    let tick2 = tick.clone();
    let tick_send = send_wrapper::SendWrapper::new(tick2);
    *tick.borrow_mut() = Some(Box::new(move || {
        let now = window().performance().map(|p| p.now()).unwrap_or(0.0);
        let dt = ((now - last_t.get_value()) / 1000.0).clamp(0.0, 0.1) as f32;
        last_t.set_value(now);
        let playing = app.peek(|e| e.timeline.playing);
        if !playing {
            return;
        }
        app.view(|e| e.timeline.tick(dt));
        app.rev.update(|r| *r += 1);
        let t = tick_send.clone();
        request_animation_frame(move || {
            if let Some(f) = t.borrow().as_ref() {
                f();
            }
        });
    }));
    let tick_start = send_wrapper::SendWrapper::new(tick);
    let play = move |reverse: bool| {
        let now = window().performance().map(|p| p.now()).unwrap_or(0.0);
        last_t.set_value(now);
        let start = app.peek(|e| !e.timeline.playing || e.timeline.reverse != reverse);
        app.view(|e| {
            if e.timeline.playing && e.timeline.reverse == reverse {
                e.timeline.playing = false;
            } else {
                e.timeline.playing = true;
                e.timeline.reverse = reverse;
            }
        });
        app.rev.update(|r| *r += 1);
        if start {
            let t = tick_start.clone();
            request_animation_frame(move || {
                if let Some(f) = t.borrow().as_ref() {
                    f();
                }
            });
        }
    };
    let play2 = play.clone();
    let step = move |f: fn(&mut forma3d_core::editor::Timeline)| {
        app.view(|e| {
            e.timeline.playing = false;
            f(&mut e.timeline);
        });
        app.rev.update(|r| *r += 1);
    };

    view! {
        <div class="timeline">
            <div
                class="ruler"
                node_ref=ruler
                on:pointerdown=move |ev: web_sys::PointerEvent| {
                    scrubbing.set_value(true);
                    if let Some(t) = ev.target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok()) {
                        let _ = t.set_pointer_capture(ev.pointer_id());
                    }
                    frame_at(ev.client_x() as f64);
                }
                on:pointermove=move |ev: web_sys::PointerEvent| {
                    if scrubbing.get_value() {
                        frame_at(ev.client_x() as f64);
                    }
                }
                on:pointerup=move |_| scrubbing.set_value(false)
            >
                {move || {
                    let (lo, hi, _, _, _) = range();
                    let span = (hi - lo).max(1);
                    let every = if span > 400 { 50 } else if span > 150 { 20 } else if span > 60 { 10 } else { 5 };
                    (lo..=hi)
                        .filter(|f| f % (every / 5).max(1) == 0)
                        .map(|f| {
                            let x = (f - lo) as f64 / span as f64 * 100.0;
                            let big = f % every == 0;
                            view! {
                                <i style=format!("left:{x}%;height:{}px", if big { 9 } else { 5 })></i>
                                {(big && f != hi).then(|| view! { <b style=format!("left:{x}%")>{f}</b> })}
                            }
                        })
                        .collect_view()
                }}
                <div class="playhead" style=move || {
                    let (lo, hi, f, _, _) = range();
                    format!("left:{}%", (f - lo) as f64 / (hi - lo).max(1) as f64 * 100.0)
                }></div>
            </div>
            <div class="transport">
                <span class="field" title="Current frame">{move || format!("{} F", range().2)}</span>
                <span class="field" title="First frame">{move || format!("{} F", range().0)}</span>
                <div class="tp">
                    <button class="tb" title="Go to start" on:click=move |_| step(|t| t.frame = t.min)>{icon("first")}</button>
                    <button class="tb" title="Previous frame" on:click=move |_| step(|t| t.frame = (t.frame - 1).max(t.min))>{icon("prev")}</button>
                    <button class="tb" title="Play backwards" on:click=move |_| play2(true)>{icon("back")}</button>
                    <button class="tb play" title="Play / pause" on:click=move |_| play(false)>
                        {move || if range().4 { icon("pause").into_any() } else { icon("play").into_any() }}
                    </button>
                    <button class="tb" title="Next frame" on:click=move |_| step(|t| t.frame = (t.frame + 1).min(t.max))>{icon("next")}</button>
                    <button class="tb" title="Go to end" on:click=move |_| step(|t| t.frame = t.max)>{icon("last")}</button>
                </div>
                <span style="color:var(--dim)">{move || format!("{} fps", range().3)}</span>
                <span class="field" title="Last frame">{move || format!("{} F", range().1)}</span>
            </div>
        </div>
    }
}
