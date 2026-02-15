// Author: Dustin Pilgrim
// License: MIT

pub const BORDER_THICKNESS: i32 = 2;
pub const HANDLE_SIZE: i32 = 8;
pub const HANDLE_HIT: i32 = 12;
pub const MIN_W: i32 = 8;
pub const MIN_H: i32 = 8;

#[derive(Clone, Copy, Debug, Default)]
pub struct RectLocal {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl RectLocal {
    pub fn clamp_to(&mut self, min_x: i32, min_y: i32, max_x: i32, max_y: i32) {
        self.w = self.w.max(MIN_W);
        self.h = self.h.max(MIN_H);
        if self.x < min_x {
            self.x = min_x;
        }
        if self.y < min_y {
            self.y = min_y;
        }
        if self.x + self.w > max_x {
            self.x = (max_x - self.w).max(min_x);
        }
        if self.y + self.h > max_y {
            self.y = (max_y - self.h).max(min_y);
        }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < (self.x + self.w) && py < (self.y + self.h)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum DragMode {
    None,
    Move,
    Resize(ResizeDir),
}

#[derive(Clone, Copy, Debug)]
pub struct ResizeDir {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl ResizeDir {
    pub fn any(&self) -> bool {
        self.left || self.right || self.top || self.bottom
    }
}

pub fn hit_test(selection: RectLocal, px: i32, py: i32) -> DragMode {
    let r = selection;

    let left =
        (px - r.x).abs() <= HANDLE_HIT && py >= r.y - HANDLE_HIT && py <= r.y + r.h + HANDLE_HIT;
    let right = (px - (r.x + r.w)).abs() <= HANDLE_HIT
        && py >= r.y - HANDLE_HIT
        && py <= r.y + r.h + HANDLE_HIT;
    let top =
        (py - r.y).abs() <= HANDLE_HIT && px >= r.x - HANDLE_HIT && px <= r.x + r.w + HANDLE_HIT;
    let bottom = (py - (r.y + r.h)).abs() <= HANDLE_HIT
        && px >= r.x - HANDLE_HIT
        && px <= r.x + r.w + HANDLE_HIT;

    let dir = ResizeDir {
        left,
        right,
        top,
        bottom,
    };

    if dir.any() {
        return DragMode::Resize(dir);
    }

    if r.contains(px, py) {
        DragMode::Move
    } else {
        // default drag = bottom-right resize
        DragMode::Resize(ResizeDir {
            left: false,
            right: true,
            top: false,
            bottom: true,
        })
    }
}

pub fn apply_drag(
    drag_mode: DragMode,
    cursor: (i32, i32),
    grab_cursor: (i32, i32),
    grab_rect: RectLocal,
    desktop_min_x: i32,
    desktop_min_y: i32,
    desktop_max_x: i32,
    desktop_max_y: i32,
) -> RectLocal {
    let (cx, cy) = cursor;
    let dx = cx - grab_cursor.0;
    let dy = cy - grab_cursor.1;

    match drag_mode {
        DragMode::None => grab_rect,

        DragMode::Move => {
            let mut r = grab_rect;
            r.x += dx;
            r.y += dy;
            r.clamp_to(desktop_min_x, desktop_min_y, desktop_max_x, desktop_max_y);
            r
        }

        DragMode::Resize(dir) => {
            let mut left = grab_rect.x;
            let mut right = grab_rect.x + grab_rect.w;
            let mut top = grab_rect.y;
            let mut bottom = grab_rect.y + grab_rect.h;

            if dir.left {
                left = cx;
            }
            if dir.right {
                right = cx;
            }
            if dir.top {
                top = cy;
            }
            if dir.bottom {
                bottom = cy;
            }

            if left > right {
                std::mem::swap(&mut left, &mut right);
            }
            if top > bottom {
                std::mem::swap(&mut top, &mut bottom);
            }

            let mut r = RectLocal {
                x: left,
                y: top,
                w: (right - left).max(MIN_W),
                h: (bottom - top).max(MIN_H),
            };

            r.clamp_to(desktop_min_x, desktop_min_y, desktop_max_x, desktop_max_y);
            r
        }
    }
}
