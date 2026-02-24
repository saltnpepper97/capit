// Author: Dustin Pilgrim
// License: MIT

pub const BORDER_THICKNESS: i32 = 2;

// Bigger circles
pub const HANDLE_SIZE: i32 = 12;
pub const HANDLE_HIT: i32 = 14;

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

fn dist2(ax: i32, ay: i32, bx: i32, by: i32) -> i64 {
    let dx = (ax - bx) as i64;
    let dy = (ay - by) as i64;
    dx * dx + dy * dy
}

fn corner_hit(selection: RectLocal, px: i32, py: i32) -> Option<ResizeDir> {
    // Corner handles are centered on the rectangle corners.
    // Use a circular hit region so grab feels "round".
    let r = selection;
    let rad = HANDLE_HIT.max(HANDLE_SIZE / 2);
    let rad2 = (rad as i64) * (rad as i64);

    let tl = dist2(px, py, r.x, r.y);
    let tr = dist2(px, py, r.x + r.w, r.y);
    let bl = dist2(px, py, r.x, r.y + r.h);
    let br = dist2(px, py, r.x + r.w, r.y + r.h);

    let mut best = (i64::MAX, 0);
    for (d, idx) in [(tl, 0), (tr, 1), (bl, 2), (br, 3)] {
        if d < best.0 {
            best = (d, idx);
        }
    }

    if best.0 <= rad2 {
        let dir = match best.1 {
            0 => ResizeDir {
                left: true,
                right: false,
                top: true,
                bottom: false,
            }, // TL
            1 => ResizeDir {
                left: false,
                right: true,
                top: true,
                bottom: false,
            }, // TR
            2 => ResizeDir {
                left: true,
                right: false,
                top: false,
                bottom: true,
            }, // BL
            _ => ResizeDir {
                left: false,
                right: true,
                top: false,
                bottom: true,
            }, // BR
        };
        return Some(dir);
    }

    None
}

fn nearest_corner_dir(selection: RectLocal, px: i32, py: i32) -> ResizeDir {
    let r = selection;
    let tl = dist2(px, py, r.x, r.y);
    let tr = dist2(px, py, r.x + r.w, r.y);
    let bl = dist2(px, py, r.x, r.y + r.h);
    let br = dist2(px, py, r.x + r.w, r.y + r.h);

    let mut best = (tl, 0);
    for (d, idx) in [(tr, 1), (bl, 2), (br, 3)] {
        if d < best.0 {
            best = (d, idx);
        }
    }

    match best.1 {
        0 => ResizeDir {
            left: true,
            right: false,
            top: true,
            bottom: false,
        },
        1 => ResizeDir {
            left: false,
            right: true,
            top: true,
            bottom: false,
        },
        2 => ResizeDir {
            left: true,
            right: false,
            top: false,
            bottom: true,
        },
        _ => ResizeDir {
            left: false,
            right: true,
            top: false,
            bottom: true,
        },
    }
}

pub fn hit_test(selection: RectLocal, px: i32, py: i32) -> DragMode {
    let r = selection;

    // 1) Corners first (circular grab zones)
    if let Some(dir) = corner_hit(r, px, py) {
        return DragMode::Resize(dir);
    }

    // 2) Edges (band hit zones)
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

    // 3) Inside = move
    if r.contains(px, py) {
        DragMode::Move
    } else {
        // 4) Outside = resize nearest corner
        DragMode::Resize(nearest_corner_dir(r, px, py))
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
