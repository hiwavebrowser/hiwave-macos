//! CSS transitions (css-transitions-1), run by the engine (S13a).
//!
//! Until now the cascade parsed `transition` and nothing ticked it: a
//! `transition: opacity 100ms` class toggle jumped to its end value and no
//! `transitionend` ever fired. Now every build hands each element's
//! cascaded style to [`Animations::on_style`], which keeps the values of
//! the properties the element's `transition` names as of the last build
//! (the before-change style), starts a transition for each one that
//! changed (from the value the element shows now, so a toggle back midway
//! reverses from where it got to, with the spec's reversing shortening),
//! and overlays the running transitions' values on the style. The layout
//! box, paint and `getComputedStyle` all read the overlaid value; the
//! cascade's own result (and the style memo) stay untouched.
//!
//! The clock is the page's timer clock, read after each live turn's timers
//! (`DomBindings::clock_now_ms`), so transitions move with
//! `requestAnimationFrame` and the tests' `pump`. While one runs, the live
//! loop lays the view out every frame (`Engine::settle_animations`).
//!
//! Events (`transitionrun`, `transitionstart`, `transitionend`,
//! `transitioncancel`) are queued here as the phases change and fired at
//! the element after the build, in queue order, as Chrome orders them.
//!
//! What is interpolated: numbers (`opacity`), lengths of one unit (`px`
//! with `px`, `%` with `%`; `auto` never transitions, as in Chrome),
//! colours (premultiplied sRGB), and transform lists (function by function
//! when the lists match, `none` as the identity of the other list, else
//! the two matrices component-wise, which is not the spec's decomposition
//! and is a stated gap). Not here yet: the individual `translate`/
//! `rotate`/`scale` properties, `border-radius`, `box-shadow`,
//! `background-position`, `line-height`, `visibility`'s discrete rule,
//! pseudo-element transitions, and `transition-behavior: allow-discrete`.

use rustkit_css::{
    Color, ComputedStyle, Length, TimingFunction, TransformList, TransformOp, TransitionItem,
};
use std::collections::HashMap;

/// A value one of the transitionable properties takes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AnimValue {
    Number(f32),
    Length(Length),
    Color(Color),
    Transform(TransformList),
}

struct Prop {
    name: &'static str,
    get: fn(&ComputedStyle) -> AnimValue,
    set: fn(&mut ComputedStyle, &AnimValue),
}

macro_rules! num_prop {
    ($name:literal, $field:ident) => {
        Prop {
            name: $name,
            get: |s| AnimValue::Number(s.$field),
            set: |s, v| {
                if let AnimValue::Number(n) = v {
                    s.$field = *n;
                }
            },
        }
    };
}
macro_rules! len_prop {
    ($name:literal, $field:ident) => {
        Prop {
            name: $name,
            get: |s| AnimValue::Length(s.$field.clone()),
            set: |s, v| {
                if let AnimValue::Length(l) = v {
                    s.$field = l.clone();
                }
            },
        }
    };
}
macro_rules! inset_prop {
    ($name:literal, $field:ident) => {
        Prop {
            name: $name,
            get: |s| AnimValue::Length(s.$field.clone().unwrap_or(Length::Auto)),
            set: |s, v| {
                if let AnimValue::Length(l) = v {
                    s.$field = Some(l.clone());
                }
            },
        }
    };
}
macro_rules! color_prop {
    ($name:literal, $field:ident) => {
        Prop {
            name: $name,
            get: |s| AnimValue::Color(s.$field),
            set: |s, v| {
                if let AnimValue::Color(c) = v {
                    s.$field = *c;
                }
            },
        }
    };
}

/// The properties a transition can run on, by their CSS names.
const PROPS: &[Prop] = &[
    num_prop!("opacity", opacity),
    Prop {
        name: "transform",
        get: |s| AnimValue::Transform(s.transform.clone()),
        set: |s, v| {
            if let AnimValue::Transform(t) = v {
                s.transform = t.clone();
            }
        },
    },
    color_prop!("color", color),
    color_prop!("background-color", background_color),
    color_prop!("border-top-color", border_top_color),
    color_prop!("border-right-color", border_right_color),
    color_prop!("border-bottom-color", border_bottom_color),
    color_prop!("border-left-color", border_left_color),
    len_prop!("width", width),
    len_prop!("height", height),
    len_prop!("min-width", min_width),
    len_prop!("min-height", min_height),
    len_prop!("max-width", max_width),
    len_prop!("max-height", max_height),
    len_prop!("margin-top", margin_top),
    len_prop!("margin-right", margin_right),
    len_prop!("margin-bottom", margin_bottom),
    len_prop!("margin-left", margin_left),
    len_prop!("padding-top", padding_top),
    len_prop!("padding-right", padding_right),
    len_prop!("padding-bottom", padding_bottom),
    len_prop!("padding-left", padding_left),
    len_prop!("border-top-width", border_top_width),
    len_prop!("border-right-width", border_right_width),
    len_prop!("border-bottom-width", border_bottom_width),
    len_prop!("border-left-width", border_left_width),
    inset_prop!("top", top),
    inset_prop!("right", right),
    inset_prop!("bottom", bottom),
    inset_prop!("left", left),
    len_prop!("font-size", font_size),
    len_prop!("letter-spacing", letter_spacing),
    len_prop!("word-spacing", word_spacing),
    len_prop!("text-indent", text_indent),
];

/// The shorthands a `transition-property` name expands to.
fn expands_to(name: &str) -> &'static [&'static str] {
    match name {
        "all" => &[],
        "margin" => &["margin-top", "margin-right", "margin-bottom", "margin-left"],
        "padding" => &[
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
        ],
        "border-color" => &[
            "border-top-color",
            "border-right-color",
            "border-bottom-color",
            "border-left-color",
        ],
        "border-width" => &[
            "border-top-width",
            "border-right-width",
            "border-bottom-width",
            "border-left-width",
        ],
        "border" => &[
            "border-top-color",
            "border-right-color",
            "border-bottom-color",
            "border-left-color",
            "border-top-width",
            "border-right-width",
            "border-bottom-width",
            "border-left-width",
        ],
        "inset" => &["top", "right", "bottom", "left"],
        "background" => &["background-color"],
        "font" => &["font-size"],
        _ => &[],
    }
}

/// The transition item that applies to property `prop` (the last one that
/// names it, or `all`), with a combined duration above zero.
fn spec_for<'a>(items: &'a [TransitionItem], prop: &str) -> Option<&'a TransitionItem> {
    items.iter().rev().find(|item| {
        let name = item.property.trim();
        name.eq_ignore_ascii_case("all")
            || name.eq_ignore_ascii_case(prop)
            || expands_to(&name.to_ascii_lowercase()).contains(&prop)
    })
}

/// Split `value` at `sep` outside parentheses (`cubic-bezier(0, 0, 1, 1)`
/// stays whole).
pub(crate) fn split_top_level(value: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in value.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            c if c == sep && depth == 0 => {
                out.push(value[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(value[start..].trim());
    out.retain(|s| !s.is_empty());
    out
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// `a` and `b` interpolated at `t`; `None` when they are not interpolable
/// (css-transitions-1 §3: no transition then starts).
fn interpolate_length(a: &Length, b: &Length, t: f32) -> Option<Length> {
    use Length::*;
    Some(match (a, b) {
        (Px(x), Px(y)) => Px(lerp(*x, *y, t)),
        (Zero, Px(y)) => Px(lerp(0.0, *y, t)),
        (Px(x), Zero) => Px(lerp(*x, 0.0, t)),
        (Percent(x), Percent(y)) => Percent(lerp(*x, *y, t)),
        (Zero, Percent(y)) => Percent(lerp(0.0, *y, t)),
        (Percent(x), Zero) => Percent(lerp(*x, 0.0, t)),
        (Em(x), Em(y)) => Em(lerp(*x, *y, t)),
        (Rem(x), Rem(y)) => Rem(lerp(*x, *y, t)),
        (Vw(x), Vw(y)) => Vw(lerp(*x, *y, t)),
        (Vh(x), Vh(y)) => Vh(lerp(*x, *y, t)),
        (Vmin(x), Vmin(y)) => Vmin(lerp(*x, *y, t)),
        (Vmax(x), Vmax(y)) => Vmax(lerp(*x, *y, t)),
        (Zero, Zero) => Zero,
        _ => return None,
    })
}

/// Premultiplied sRGB interpolation (css-color-4 §12, what Chrome does for
/// `color` and the other legacy-colour properties).
fn interpolate_color(a: &Color, b: &Color, t: f32) -> Color {
    let alpha = lerp(a.a, b.a, t);
    if alpha <= 0.0 {
        return Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        };
    }
    let channel = |x: u8, y: u8| {
        let v = lerp(x as f32 * a.a, y as f32 * b.a, t) / alpha;
        v.round().clamp(0.0, 255.0) as u8
    };
    Color {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: alpha,
    }
}

fn identity_of(op: &TransformOp) -> TransformOp {
    use TransformOp::*;
    match op {
        Translate(..) => Translate(Length::Zero, Length::Zero),
        TranslateX(_) => TranslateX(Length::Zero),
        TranslateY(_) => TranslateY(Length::Zero),
        Scale(..) => Scale(1.0, 1.0),
        ScaleX(_) => ScaleX(1.0),
        ScaleY(_) => ScaleY(1.0),
        Rotate(_) => Rotate(0.0),
        SkewX(_) => SkewX(0.0),
        SkewY(_) => SkewY(0.0),
        Skew(..) => Skew(0.0, 0.0),
        Matrix(..) => Matrix(1.0, 0.0, 0.0, 1.0, 0.0, 0.0),
    }
}

fn interpolate_op(a: &TransformOp, b: &TransformOp, t: f32) -> Option<TransformOp> {
    use TransformOp::*;
    Some(match (a, b) {
        (Translate(ax, ay), Translate(bx, by)) => Translate(
            interpolate_length(ax, bx, t)?,
            interpolate_length(ay, by, t)?,
        ),
        (TranslateX(ax), TranslateX(bx)) => TranslateX(interpolate_length(ax, bx, t)?),
        (TranslateY(ay), TranslateY(by)) => TranslateY(interpolate_length(ay, by, t)?),
        (Scale(ax, ay), Scale(bx, by)) => Scale(lerp(*ax, *bx, t), lerp(*ay, *by, t)),
        (ScaleX(x), ScaleX(y)) => ScaleX(lerp(*x, *y, t)),
        (ScaleY(x), ScaleY(y)) => ScaleY(lerp(*x, *y, t)),
        (Rotate(x), Rotate(y)) => Rotate(lerp(*x, *y, t)),
        (SkewX(x), SkewX(y)) => SkewX(lerp(*x, *y, t)),
        (SkewY(x), SkewY(y)) => SkewY(lerp(*x, *y, t)),
        (Skew(ax, ay), Skew(bx, by)) => Skew(lerp(*ax, *bx, t), lerp(*ay, *by, t)),
        (Matrix(a0, a1, a2, a3, a4, a5), Matrix(b0, b1, b2, b3, b4, b5)) => Matrix(
            lerp(*a0, *b0, t),
            lerp(*a1, *b1, t),
            lerp(*a2, *b2, t),
            lerp(*a3, *b3, t),
            lerp(*a4, *b4, t),
            lerp(*a5, *b5, t),
        ),
        _ => return None,
    })
}

/// css-transforms-1 §9: matching lists interpolate function by function,
/// `none` is the identity of the other list, and anything else falls back
/// to the two matrices (interpolated component-wise here, not decomposed).
fn interpolate_transform(a: &TransformList, b: &TransformList, t: f32) -> TransformList {
    let (a_ops, b_ops): (Vec<TransformOp>, Vec<TransformOp>) =
        match (a.ops.is_empty(), b.ops.is_empty()) {
            (true, true) => return TransformList::none(),
            (true, false) => (b.ops.iter().map(identity_of).collect(), b.ops.clone()),
            (false, true) => (a.ops.clone(), a.ops.iter().map(identity_of).collect()),
            (false, false) => (a.ops.clone(), b.ops.clone()),
        };
    if a_ops.len() == b_ops.len() {
        let ops: Option<Vec<TransformOp>> = a_ops
            .iter()
            .zip(&b_ops)
            .map(|(x, y)| interpolate_op(x, y, t))
            .collect();
        if let Some(ops) = ops {
            return TransformList { ops };
        }
    }
    let ma = a.to_matrix(0.0, 0.0);
    let mb = b.to_matrix(0.0, 0.0);
    TransformList {
        ops: vec![TransformOp::Matrix(
            lerp(ma[0], mb[0], t),
            lerp(ma[1], mb[1], t),
            lerp(ma[2], mb[2], t),
            lerp(ma[3], mb[3], t),
            lerp(ma[4], mb[4], t),
            lerp(ma[5], mb[5], t),
        )],
    }
}

fn interpolate(a: &AnimValue, b: &AnimValue, t: f32) -> Option<AnimValue> {
    Some(match (a, b) {
        (AnimValue::Number(x), AnimValue::Number(y)) => AnimValue::Number(lerp(*x, *y, t)),
        (AnimValue::Length(x), AnimValue::Length(y)) => {
            AnimValue::Length(interpolate_length(x, y, t)?)
        }
        (AnimValue::Color(x), AnimValue::Color(y)) => AnimValue::Color(interpolate_color(x, y, t)),
        (AnimValue::Transform(x), AnimValue::Transform(y)) => {
            AnimValue::Transform(interpolate_transform(x, y, t))
        }
        _ => return None,
    })
}

/// One running transition (css-transitions-1 §3).
#[derive(Clone, Debug)]
struct Transition {
    node: usize,
    prop: usize,
    from: AnimValue,
    to: AnimValue,
    /// When the delay was counted from.
    created_ms: f64,
    delay_ms: f64,
    duration_ms: f64,
    timing: TimingFunction,
    reversing_adjusted_start: AnimValue,
    reversing_shortening: f32,
    /// `transitionstart` has fired (the active phase began).
    started: bool,
    done: bool,
}

impl Transition {
    fn start_ms(&self) -> f64 {
        self.created_ms + self.delay_ms
    }
    fn end_ms(&self) -> f64 {
        self.start_ms() + self.duration_ms
    }
    /// Input progress at `now`, 0 before the active phase.
    fn progress(&self, now: f64) -> f32 {
        if self.duration_ms <= 0.0 {
            return if now >= self.start_ms() { 1.0 } else { 0.0 };
        }
        (((now - self.start_ms()) / self.duration_ms) as f32).clamp(0.0, 1.0)
    }
    fn value_at(&self, now: f64) -> AnimValue {
        let eased = self.timing.evaluate(self.progress(now));
        interpolate(&self.from, &self.to, eased).unwrap_or_else(|| self.to.clone())
    }
    /// `elapsedTime` of `transitionrun`/`transitionstart`.
    fn elapsed_at_start(&self) -> f32 {
        ((-self.delay_ms).max(0.0).min(self.duration_ms) / 1000.0) as f32
    }
    /// `elapsedTime` of `transitioncancel`: the active time at `now`.
    fn elapsed_at(&self, now: f64) -> f32 {
        ((now - self.start_ms()).clamp(0.0, self.duration_ms) / 1000.0) as f32
    }
}

/// A transition event to fire at an element once the build is over.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnimationEvent {
    pub(crate) node: usize,
    pub(crate) kind: &'static str,
    pub(crate) property: String,
    pub(crate) elapsed_s: f32,
}

/// The view's running transitions and what they need between builds.
#[derive(Default)]
pub(crate) struct Animations {
    /// The animation clock, ms on the page's timer clock.
    pub(crate) clock_ms: f64,
    /// The clock the last build ran at (`None` before any).
    pub(crate) last_build_clock: Option<f64>,
    /// Transitions do not start (parity captures): values jump.
    pub(crate) disabled: bool,
    /// The transitionable values each element with a `transition` cascaded
    /// to at the last build, by raw NodeId and property index.
    base: HashMap<usize, Vec<(usize, AnimValue)>>,
    transitions: Vec<Transition>,
    events: Vec<AnimationEvent>,
    /// Property names a `transition` named that the engine cannot run,
    /// once each per document, for the Inspector's unsupported APIs.
    reported: std::collections::BTreeSet<String>,
    pending_unsupported: Vec<String>,
}

impl Animations {
    /// A new document: nothing of the old one's transitions carries over.
    pub(crate) fn reset(&mut self) {
        *self = Animations {
            disabled: self.disabled,
            ..Animations::default()
        };
    }

    /// The transition property names reported since the last call
    /// (`CSSTransition.<name>` in the Inspector's unsupported APIs).
    pub(crate) fn take_unsupported(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_unsupported)
    }

    fn note_unsupported(&mut self, specs: &[TransitionItem]) {
        for item in specs {
            let name = item.property.trim().to_ascii_lowercase();
            if name == "all"
                || name == "none"
                || PROPS.iter().any(|p| p.name == name)
                || !expands_to(&name).is_empty()
                || self.reported.contains(&name)
            {
                continue;
            }
            self.reported.insert(name.clone());
            self.pending_unsupported.push(name);
        }
    }

    /// Whether the live loop has to keep turning for this view: a
    /// transition is running or an event waits to be fired.
    pub(crate) fn active(&self) -> bool {
        !self.transitions.is_empty() || !self.events.is_empty()
    }

    pub(crate) fn take_events(&mut self) -> Vec<AnimationEvent> {
        std::mem::take(&mut self.events)
    }

    fn push_event(&mut self, t: &Transition, kind: &'static str, elapsed_s: f32) {
        self.events.push(AnimationEvent {
            node: t.node,
            kind,
            property: PROPS[t.prop].name.to_string(),
            elapsed_s,
        });
    }

    /// Cancel the transitions of elements that `alive` no longer finds in
    /// the document (css-transitions-1 §3: an element removed from the
    /// document ends its transitions with `transitioncancel`).
    pub(crate) fn prune(&mut self, alive: impl Fn(usize) -> bool) {
        let now = self.clock_ms;
        let mut gone = Vec::new();
        self.transitions.retain(|t| {
            if alive(t.node) {
                true
            } else {
                gone.push(t.clone());
                false
            }
        });
        for t in gone {
            self.push_event(&t, "transitioncancel", t.elapsed_at(now));
            self.base.remove(&t.node);
        }
    }

    /// The build's hand-off for one element: start the transitions its
    /// changed values call for and overlay the running ones on `style`.
    pub(crate) fn on_style(&mut self, node: usize, style: &mut ComputedStyle) {
        let now = self.clock_ms;
        let has_running = self.transitions.iter().any(|t| t.node == node);
        if style.transitions.is_empty() && !has_running {
            self.base.remove(&node);
            return;
        }
        let specs = style.transitions.clone();
        self.note_unsupported(&specs);
        let covered: Vec<usize> = (0..PROPS.len())
            .filter(|&i| spec_for(&specs, PROPS[i].name).is_some())
            .collect();
        let new_vals: Vec<(usize, AnimValue)> = covered
            .iter()
            .map(|&i| (i, (PROPS[i].get)(style)))
            .collect();
        let prev = self.base.insert(node, new_vals.clone());

        // The transition's spec went away (`transition: none`, or the
        // property is no longer named): the running ones are cancelled.
        let mut cancelled = Vec::new();
        self.transitions.retain(|t| {
            if t.node == node && !covered.contains(&t.prop) {
                cancelled.push(t.clone());
                false
            } else {
                true
            }
        });
        for t in cancelled {
            self.push_event(&t, "transitioncancel", t.elapsed_at(now));
        }

        if let Some(prev) = prev.filter(|_| !self.disabled) {
            for (i, new) in &new_vals {
                let Some(old) = prev.iter().find(|(j, _)| j == i).map(|(_, v)| v) else {
                    continue;
                };
                let Some(spec) = spec_for(&specs, PROPS[*i].name) else {
                    continue;
                };
                let running = self
                    .transitions
                    .iter()
                    .position(|t| t.node == node && t.prop == *i);
                // Already heading there: the running one keeps going.
                if let Some(at) = running {
                    if self.transitions[at].to == *new {
                        continue;
                    }
                } else if old == new {
                    continue;
                }
                // To the microsecond: `0.1` seconds as an f32 is 100.0000015
                // ms as an f64, and an end at exactly 1100 would be missed.
                let us = |ms: f64| (ms * 1000.0).round() / 1000.0;
                let duration_ms = us((spec.duration.max(0.0) as f64) * 1000.0);
                let delay_ms = us(spec.delay as f64 * 1000.0);
                // The before-change value: what the element shows now.
                let (from, reversing) = match running {
                    Some(at) => {
                        let t = self.transitions.remove(at);
                        let current = t.value_at(now);
                        self.push_event(&t, "transitioncancel", t.elapsed_at(now));
                        (current, Some(t))
                    }
                    None => (old.clone(), None),
                };
                if duration_ms + delay_ms.max(0.0) <= 0.0 || interpolate(&from, new, 0.5).is_none()
                {
                    continue;
                }
                // css-transitions-1 §3, the reversing shortening factor:
                // a transition back to where the cancelled one started
                // takes as long as that one had got.
                let (reversing_adjusted_start, factor) = match reversing {
                    Some(t) if t.reversing_adjusted_start == *new => {
                        let p = t.timing.evaluate(t.progress(now));
                        let f = (p * t.reversing_shortening + (1.0 - t.reversing_shortening))
                            .abs()
                            .clamp(0.0, 1.0);
                        (t.to.clone(), f)
                    }
                    _ => (from.clone(), 1.0),
                };
                let delay_ms = if delay_ms < 0.0 {
                    us(delay_ms * factor as f64)
                } else {
                    delay_ms
                };
                let t = Transition {
                    node,
                    prop: *i,
                    from,
                    to: new.clone(),
                    created_ms: now,
                    delay_ms,
                    duration_ms: us(duration_ms * factor as f64),
                    timing: spec.timing.clone(),
                    reversing_adjusted_start,
                    reversing_shortening: factor,
                    started: false,
                    done: false,
                };
                self.push_event(&t, "transitionrun", t.elapsed_at_start());
                self.transitions.push(t);
            }
        }

        // Overlay the running transitions of this element.
        let mut i = 0;
        while i < self.transitions.len() {
            if self.transitions[i].node != node {
                i += 1;
                continue;
            }
            let t = &mut self.transitions[i];
            if !t.started && now >= t.start_ms() {
                t.started = true;
                let (t, elapsed) = (t.clone(), t.elapsed_at_start());
                self.push_event(&t, "transitionstart", elapsed);
            }
            let t = &mut self.transitions[i];
            let value = t.value_at(now);
            (PROPS[t.prop].set)(style, &value);
            if now >= t.end_ms() {
                t.done = true;
                let (t, elapsed) = (t.clone(), (t.duration_ms / 1000.0) as f32);
                self.push_event(&t, "transitionend", elapsed);
                self.transitions.remove(i);
            } else {
                i += 1;
            }
        }
    }
}

/// `transition` shorthand: a comma-separated list of
/// `[property] || duration || timing-function || delay` items; the first
/// time is the duration, the second the delay. `none` is the empty list.
pub(crate) fn parse_transition_shorthand(
    value: &str,
    parse_time: fn(&str) -> Option<f32>,
    parse_timing: fn(&str) -> TimingFunction,
) -> Vec<TransitionItem> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") || value.is_empty() {
        return Vec::new();
    }
    split_top_level(value, ',')
        .into_iter()
        .map(|item| {
            let mut out = TransitionItem {
                property: "all".to_string(),
                ..Default::default()
            };
            let mut times = 0;
            for token in split_top_level(item, ' ') {
                let lower = token.to_ascii_lowercase();
                if let Some(t) = parse_time(&lower) {
                    if times == 0 {
                        out.duration = t;
                    } else {
                        out.delay = t;
                    }
                    times += 1;
                } else if is_timing_keyword(&lower) {
                    out.timing = parse_timing(&lower);
                } else if !lower.is_empty() {
                    out.property = lower;
                }
            }
            out
        })
        .collect()
}

fn is_timing_keyword(token: &str) -> bool {
    matches!(
        token,
        "ease" | "linear" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end"
    ) || token.starts_with("cubic-bezier(")
        || token.starts_with("steps(")
}

/// `transition-property`: the list sets how many items there are; the
/// other longhands' values repeat to fill them.
pub(crate) fn set_transition_properties(style: &mut ComputedStyle, value: &str) {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        style.transitions.clear();
        return;
    }
    let names = split_top_level(value, ',');
    let old = std::mem::take(&mut style.transitions);
    style.transitions = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let mut item = if old.is_empty() {
                TransitionItem::default()
            } else {
                old[i % old.len()].clone()
            };
            item.property = name.to_ascii_lowercase();
            item
        })
        .collect();
}

/// A `transition-duration`/`-delay`/`-timing-function` list over the
/// items there are (`all` items when none was named yet).
pub(crate) fn set_transition_component<T: Clone>(
    style: &mut ComputedStyle,
    values: Vec<T>,
    set: fn(&mut TransitionItem, T),
) {
    if values.is_empty() {
        return;
    }
    if style.transitions.is_empty() {
        style.transitions = values
            .iter()
            .map(|_| TransitionItem {
                property: "all".to_string(),
                ..Default::default()
            })
            .collect();
    }
    let n = values.len();
    for (i, item) in style.transitions.iter_mut().enumerate() {
        set(item, values[i % n].clone());
    }
}

/// The scalar transition fields follow the first item (what
/// `getComputedStyle` serializes).
pub(crate) fn sync_transition_scalars(style: &mut ComputedStyle, none: bool) {
    match style.transitions.first() {
        Some(item) => {
            style.transition_property = style
                .transitions
                .iter()
                .map(|item| item.property.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            style.transition_duration = item.duration;
            style.transition_delay = item.delay;
            style.transition_timing_function = item.timing.clone();
        }
        None => {
            style.transition_property = if none {
                "none".to_string()
            } else {
                String::new()
            };
            style.transition_duration = 0.0;
            style.transition_delay = 0.0;
            style.transition_timing_function = TimingFunction::Ease;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_interpolate_premultiplied() {
        let blue = Color {
            r: 0,
            g: 0,
            b: 255,
            a: 1.0,
        };
        let red = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 1.0,
        };
        let mid = interpolate_color(&blue, &red, 0.5);
        assert_eq!((mid.r, mid.g, mid.b), (128, 0, 128));
        assert_eq!(mid.a, 1.0);
        let clear = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        };
        let mid = interpolate_color(&clear, &red, 0.5);
        assert_eq!(
            (mid.r, mid.g, mid.b),
            (255, 0, 0),
            "transparent black does not darken"
        );
        assert_eq!(mid.a, 0.5);
    }

    #[test]
    fn lengths_interpolate_only_within_a_unit() {
        assert_eq!(
            interpolate_length(&Length::Px(100.0), &Length::Px(200.0), 0.5),
            Some(Length::Px(150.0))
        );
        assert_eq!(
            interpolate_length(&Length::Zero, &Length::Px(100.0), 0.25),
            Some(Length::Px(25.0))
        );
        assert_eq!(
            interpolate_length(&Length::Auto, &Length::Px(100.0), 0.5),
            None
        );
        assert_eq!(
            interpolate_length(&Length::Percent(50.0), &Length::Px(100.0), 0.5),
            None
        );
    }

    #[test]
    fn transforms_interpolate_function_by_function_and_none_as_identity() {
        let a = TransformList {
            ops: vec![TransformOp::TranslateX(Length::Px(0.0))],
        };
        let b = TransformList {
            ops: vec![TransformOp::TranslateX(Length::Px(100.0))],
        };
        assert_eq!(
            interpolate_transform(&a, &b, 0.5).ops,
            vec![TransformOp::TranslateX(Length::Px(50.0))]
        );
        let scale = TransformList {
            ops: vec![TransformOp::Scale(2.0, 2.0)],
        };
        assert_eq!(
            interpolate_transform(&TransformList::none(), &scale, 0.5).ops,
            vec![TransformOp::Scale(1.5, 1.5)]
        );
    }

    #[test]
    fn the_shorthand_takes_a_list_with_a_timing_function_and_delay() {
        let items = parse_transition_shorthand(
            "opacity 100ms linear, transform .2s cubic-bezier(0.4, 0, 0.2, 1) 50ms, 1s",
            crate::parse_time,
            crate::parse_timing_function,
        );
        assert_eq!(items.len(), 3);
        assert_eq!(
            (
                items[0].property.as_str(),
                items[0].duration,
                items[0].delay
            ),
            ("opacity", 0.1, 0.0)
        );
        assert_eq!(items[0].timing, TimingFunction::Linear);
        assert_eq!(
            (
                items[1].property.as_str(),
                items[1].duration,
                items[1].delay
            ),
            ("transform", 0.2, 0.05)
        );
        assert_eq!(
            items[1].timing,
            TimingFunction::CubicBezier(0.4, 0.0, 0.2, 1.0)
        );
        assert_eq!(
            (items[2].property.as_str(), items[2].duration),
            ("all", 1.0)
        );
        assert!(parse_transition_shorthand(
            "none",
            crate::parse_time,
            crate::parse_timing_function
        )
        .is_empty());
    }

    #[test]
    fn the_later_item_naming_a_property_wins_and_shorthands_expand() {
        let items = parse_transition_shorthand(
            "all 1s, opacity 2s, margin 3s",
            crate::parse_time,
            crate::parse_timing_function,
        );
        assert_eq!(spec_for(&items, "opacity").unwrap().duration, 2.0);
        assert_eq!(spec_for(&items, "width").unwrap().duration, 1.0);
        assert_eq!(spec_for(&items, "margin-left").unwrap().duration, 3.0);
    }

    #[test]
    fn longhands_repeat_to_the_property_list() {
        let mut style = ComputedStyle::new();
        set_transition_properties(&mut style, "opacity, width, color");
        set_transition_component(&mut style, vec![0.1f32, 0.2], |item, d| item.duration = d);
        let d: Vec<f32> = style.transitions.iter().map(|i| i.duration).collect();
        assert_eq!(d, vec![0.1, 0.2, 0.1]);
        sync_transition_scalars(&mut style, false);
        assert_eq!(style.transition_property, "opacity, width, color");
        assert_eq!(style.transition_duration, 0.1);
    }

    #[test]
    fn a_change_starts_a_transition_and_the_overlay_follows_the_clock() {
        let mut anims = Animations::default();
        let mut style = ComputedStyle::new();
        style.opacity = 0.0;
        style.transitions = vec![TransitionItem {
            property: "opacity".into(),
            duration: 0.1,
            delay: 0.0,
            timing: TimingFunction::Linear,
        }];
        anims.clock_ms = 1000.0;
        anims.on_style(7, &mut style);
        assert!(!anims.active(), "the first build starts nothing");
        style.opacity = 1.0;
        anims.on_style(7, &mut style);
        assert_eq!(style.opacity, 0.0, "overlaid at the start value");
        let kinds: Vec<&str> = anims.events.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, ["transitionrun", "transitionstart"]);
        anims.take_events();
        anims.clock_ms = 1050.0;
        style.opacity = 1.0;
        anims.on_style(7, &mut style);
        assert!(
            (style.opacity - 0.5).abs() < 1e-4,
            "midway: {}",
            style.opacity
        );
        anims.clock_ms = 1100.0;
        style.opacity = 1.0;
        anims.on_style(7, &mut style);
        assert_eq!(style.opacity, 1.0);
        let kinds: Vec<&str> = anims.events.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, ["transitionend"]);
        assert_eq!(anims.events[0].elapsed_s, 0.1);
        assert!(!anims.active() || anims.events.len() == 1);
    }

    #[test]
    fn a_reversal_midway_is_shortened_to_the_distance_covered() {
        let mut anims = Animations::default();
        let mut style = ComputedStyle::new();
        style.opacity = 0.0;
        style.transitions = vec![TransitionItem {
            property: "opacity".into(),
            duration: 0.1,
            delay: 0.0,
            timing: TimingFunction::Linear,
        }];
        anims.on_style(1, &mut style);
        style.opacity = 1.0;
        anims.on_style(1, &mut style);
        anims.clock_ms = 40.0;
        style.opacity = 1.0;
        anims.on_style(1, &mut style);
        assert!((style.opacity - 0.4).abs() < 1e-4);
        // Back to 0 at 40 ms: from 0.4, over 40 ms.
        style.opacity = 0.0;
        anims.on_style(1, &mut style);
        assert!((style.opacity - 0.4).abs() < 1e-4);
        anims.clock_ms = 60.0;
        style.opacity = 0.0;
        anims.on_style(1, &mut style);
        assert!(
            (style.opacity - 0.2).abs() < 1e-4,
            "halfway back: {}",
            style.opacity
        );
        anims.clock_ms = 80.0;
        style.opacity = 0.0;
        anims.on_style(1, &mut style);
        assert_eq!(style.opacity, 0.0);
        let kinds: Vec<&str> = anims.take_events().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            [
                "transitionrun",
                "transitionstart",
                "transitioncancel",
                "transitionrun",
                "transitionstart",
                "transitionend"
            ]
        );
    }
}
