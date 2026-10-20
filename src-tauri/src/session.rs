//! The drag-session state machine behind "start dragging, then tap a key".
//!
//! Pure logic (no OS calls) so it can be unit-tested anywhere. The Windows layer feeds it raw
//! mouse/keyboard events from low-level hooks and acts on the returned `Output`.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Convert,
    Tools,
    Actions,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Modifier {
    Shift,
    Control,
    Alt,
}

impl Modifier {
    pub fn parse(s: &str) -> Option<Modifier> {
        match s.trim().to_ascii_lowercase().as_str() {
            "shift" => Some(Modifier::Shift),
            "control" | "ctrl" => Some(Modifier::Control),
            "alt" | "option" => Some(Modifier::Alt),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Modifier::Shift => "Shift",
            Modifier::Control => "Control",
            Modifier::Alt => "Alt",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bindings {
    pub convert: Modifier,
    pub tools: Modifier,
    pub actions: Modifier,
}

impl Bindings {
    fn mode_for(&self, m: Modifier) -> Option<Mode> {
        if m == self.convert {
            Some(Mode::Convert)
        } else if m == self.tools {
            Some(Mode::Tools)
        } else if m == self.actions {
            Some(Mode::Actions)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RawEvent {
    MouseDown { x: i32, y: i32 },
    MouseMove { x: i32, y: i32 },
    MouseUp,
    KeyDown(Modifier),
    KeyUp(Modifier),
    Escape,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Output {
    None,
    /// Show (or switch) the wheel. `x`,`y` = cursor position when the wheel first opened.
    ShowWheel { mode: Mode, x: i32, y: i32 },
    /// Escape pressed: hide the wheel and ignore the rest of this drag.
    HideWheel,
    /// Mouse released. If `wheel_open`, hide the overlay shortly after (the drop event arrives first).
    EndSession { wheel_open: bool },
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    Idle,
    Pressed { x0: i32, y0: i32 },
    Dragging,
}

/// Movement (px, either axis) before a press counts as a drag, so plain clicks never open the wheel.
pub const DRAG_THRESHOLD: i32 = 6;

#[derive(Debug)]
pub struct Session {
    bindings: Bindings,
    state: State,
    cursor: (i32, i32),
    wheel: Option<(Mode, i32, i32)>,
    /// modifiers currently held (to ignore auto-repeat key-downs)
    held: Vec<Modifier>,
    cancelled: bool,
}

impl Session {
    pub fn new(bindings: Bindings) -> Self {
        Session { bindings, state: State::Idle, cursor: (0, 0), wheel: None, held: Vec::new(), cancelled: false }
    }

    pub fn set_bindings(&mut self, b: Bindings) {
        self.bindings = b;
    }

    pub fn wheel_open(&self) -> bool {
        self.wheel.is_some()
    }

    pub fn dragging(&self) -> bool {
        matches!(self.state, State::Dragging)
    }

    pub fn handle(&mut self, ev: RawEvent) -> Output {
        match ev {
            RawEvent::MouseDown { x, y } => {
                self.cursor = (x, y);
                self.state = State::Pressed { x0: x, y0: y };
                self.wheel = None;
                self.cancelled = false;
                Output::None
            }
            RawEvent::MouseMove { x, y } => {
                self.cursor = (x, y);
                if let State::Pressed { x0, y0 } = self.state {
                    if (x - x0).abs() > DRAG_THRESHOLD || (y - y0).abs() > DRAG_THRESHOLD {
                        self.state = State::Dragging;
                    }
                }
                Output::None
            }
            RawEvent::MouseUp => {
                let was_active = !matches!(self.state, State::Idle);
                let open = self.wheel.is_some();
                self.state = State::Idle;
                self.wheel = None;
                self.cancelled = false;
                if was_active && open {
                    Output::EndSession { wheel_open: true }
                } else {
                    Output::None
                }
            }
            RawEvent::KeyDown(m) => {
                let repeat = self.held.contains(&m);
                if !repeat {
                    self.held.push(m);
                }
                if repeat || self.cancelled || !self.dragging() {
                    return Output::None;
                }
                let Some(mode) = self.bindings.mode_for(m) else { return Output::None };
                match self.wheel {
                    Some((cur, _, _)) if cur == mode => Output::None,
                    Some((_, x, y)) => {
                        self.wheel = Some((mode, x, y));
                        Output::ShowWheel { mode, x, y }
                    }
                    None => {
                        let (x, y) = self.cursor;
                        self.wheel = Some((mode, x, y));
                        Output::ShowWheel { mode, x, y }
                    }
                }
            }
            RawEvent::KeyUp(m) => {
                self.held.retain(|k| *k != m);
                Output::None
            }
            RawEvent::Escape => {
                if self.dragging() && (self.wheel.is_some() || !self.cancelled) {
                    let open = self.wheel.is_some();
                    self.wheel = None;
                    self.cancelled = true;
                    if open {
                        return Output::HideWheel;
                    }
                }
                Output::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Modifier::*;

    fn b() -> Bindings {
        Bindings { convert: Shift, tools: Control, actions: Alt }
    }
    fn drag(s: &mut Session) {
        s.handle(RawEvent::MouseDown { x: 100, y: 100 });
        s.handle(RawEvent::MouseMove { x: 140, y: 120 });
    }

    #[test]
    fn key_without_drag_does_nothing() {
        let mut s = Session::new(b());
        assert_eq!(s.handle(RawEvent::KeyDown(Shift)), Output::None);
        s.handle(RawEvent::KeyUp(Shift));
        s.handle(RawEvent::MouseDown { x: 0, y: 0 });
        assert_eq!(s.handle(RawEvent::KeyDown(Shift)), Output::None, "button down but not yet moved");
    }

    #[test]
    fn jitter_is_not_a_drag() {
        let mut s = Session::new(b());
        s.handle(RawEvent::MouseDown { x: 100, y: 100 });
        s.handle(RawEvent::MouseMove { x: 104, y: 103 });
        assert!(!s.dragging());
        assert_eq!(s.handle(RawEvent::KeyDown(Shift)), Output::None);
        s.handle(RawEvent::MouseMove { x: 120, y: 100 });
        assert!(s.dragging());
    }

    #[test]
    fn opens_on_key_down_at_current_cursor() {
        let mut s = Session::new(b());
        drag(&mut s);
        s.handle(RawEvent::MouseMove { x: 300, y: 250 });
        assert_eq!(s.handle(RawEvent::KeyDown(Shift)), Output::ShowWheel { mode: Mode::Convert, x: 300, y: 250 });
        assert!(s.wheel_open());
        // releasing the key keeps the wheel open; moving the mouse does not move it
        assert_eq!(s.handle(RawEvent::KeyUp(Shift)), Output::None);
        s.handle(RawEvent::MouseMove { x: 500, y: 500 });
        assert!(s.wheel_open());
    }

    #[test]
    fn each_binding_maps_to_its_mode() {
        for (k, mode) in [(Shift, Mode::Convert), (Control, Mode::Tools), (Alt, Mode::Actions)] {
            let mut s = Session::new(b());
            drag(&mut s);
            assert_eq!(s.handle(RawEvent::KeyDown(k)), Output::ShowWheel { mode, x: 140, y: 120 });
        }
    }

    #[test]
    fn key_repeat_and_same_mode_are_ignored_and_switching_keeps_position() {
        let mut s = Session::new(b());
        drag(&mut s);
        s.handle(RawEvent::KeyDown(Shift));
        assert_eq!(s.handle(RawEvent::KeyDown(Shift)), Output::None, "auto-repeat");
        s.handle(RawEvent::KeyUp(Shift));
        assert_eq!(s.handle(RawEvent::KeyDown(Shift)), Output::None, "same mode again");
        s.handle(RawEvent::KeyUp(Shift));
        s.handle(RawEvent::MouseMove { x: 400, y: 400 });
        assert_eq!(s.handle(RawEvent::KeyDown(Control)), Output::ShowWheel { mode: Mode::Tools, x: 140, y: 120 });
    }

    #[test]
    fn mouse_up_ends_session() {
        let mut s = Session::new(b());
        drag(&mut s);
        s.handle(RawEvent::KeyDown(Alt));
        assert_eq!(s.handle(RawEvent::MouseUp), Output::EndSession { wheel_open: true });
        assert!(!s.wheel_open() && !s.dragging());
        // a plain click never produces output
        s.handle(RawEvent::MouseDown { x: 1, y: 1 });
        assert_eq!(s.handle(RawEvent::MouseUp), Output::None);
    }

    #[test]
    fn escape_cancels_for_the_rest_of_the_drag() {
        let mut s = Session::new(b());
        drag(&mut s);
        s.handle(RawEvent::KeyDown(Shift));
        s.handle(RawEvent::KeyUp(Shift));
        assert_eq!(s.handle(RawEvent::Escape), Output::HideWheel);
        assert_eq!(s.handle(RawEvent::KeyDown(Control)), Output::None, "cancelled until the next press");
        s.handle(RawEvent::KeyUp(Control));
        s.handle(RawEvent::MouseUp);
        drag(&mut s);
        assert!(matches!(s.handle(RawEvent::KeyDown(Control)), Output::ShowWheel { .. }));
    }

    #[test]
    fn unbound_and_rebound_keys() {
        let mut s = Session::new(Bindings { convert: Alt, tools: Shift, actions: Control });
        drag(&mut s);
        assert!(matches!(s.handle(RawEvent::KeyDown(Alt)), Output::ShowWheel { mode: Mode::Convert, .. }));
        s.set_bindings(b());
        s.handle(RawEvent::KeyUp(Alt));
        s.handle(RawEvent::MouseUp);
        drag(&mut s);
        assert!(matches!(s.handle(RawEvent::KeyDown(Alt)), Output::ShowWheel { mode: Mode::Actions, .. }));
    }

    #[test]
    fn modifier_held_before_the_drag_starts_does_not_open() {
        let mut s = Session::new(b());
        s.handle(RawEvent::KeyDown(Control)); // e.g. Ctrl+drag to copy in Explorer
        drag(&mut s);
        assert_eq!(s.handle(RawEvent::KeyDown(Control)), Output::None, "still held, not a new press");
        s.handle(RawEvent::KeyUp(Control));
        assert!(matches!(s.handle(RawEvent::KeyDown(Control)), Output::ShowWheel { mode: Mode::Tools, .. }), "a fresh press does open it");
    }
}
