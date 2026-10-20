//! Placement of the wheel overlay window.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// Top-left for a `size` (w, h) window centred on `cursor`, kept fully inside `work`
/// (the monitor's work area, excluding the taskbar). Windows larger than the area are pinned to its top-left.
pub fn place_centered(cursor: (i32, i32), size: (i32, i32), work: Rect) -> (i32, i32) {
    let clamp = |v: i32, lo: i32, hi: i32| if hi < lo { lo } else { v.clamp(lo, hi) };
    let x = clamp(cursor.0 - size.0 / 2, work.left, work.right - size.0);
    let y = clamp(cursor.1 - size.1 / 2, work.top, work.bottom - size.1);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    const WORK: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1040 };

    #[test]
    fn centred_when_room() {
        assert_eq!(place_centered((960, 500), (640, 640), WORK), (640, 180));
    }
    #[test]
    fn clamped_at_edges() {
        assert_eq!(place_centered((10, 10), (640, 640), WORK), (0, 0));
        assert_eq!(place_centered((1915, 1035), (640, 640), WORK), (1280, 400));
    }
    #[test]
    fn secondary_monitor_with_negative_origin() {
        let left = Rect { left: -1920, top: 0, right: 0, bottom: 1080 };
        assert_eq!(place_centered((-1900, 540), (640, 640), left), (-1920, 220));
    }
    #[test]
    fn oversized_window_pins_to_top_left() {
        let small = Rect { left: 100, top: 50, right: 500, bottom: 400 };
        assert_eq!(place_centered((300, 200), (640, 640), small), (100, 50));
    }
}
