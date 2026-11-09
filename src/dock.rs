// Edge docking. A drop near an outer edge of the work area slides the card
// out that way until only a peek remains. A corner parks against both edges
// and hides by priority: top, then either side, then the bottom. A release
// past the peek springs back to it. Shared monitor edges are not docks.

use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MonitorFromPoint, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};

pub const PEEK_DIP: f32 = 8.0;
pub const THRESHOLD_DIP: f32 = 24.0;
pub const SLIDE_SECS: f32 = 0.25;
pub const HOVER_SECS: f32 = 0.20;
pub const AWAY_SECS: f32 = 0.40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    pub fn all() -> Self {
        Self { left: true, right: true, top: true, bottom: true }
    }

    pub fn allows(self, side: Side) -> bool {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
            Side::Top => self.top,
            Side::Bottom => self.bottom,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub edge: Side,
    pub corner: Option<Side>,
    /// Card top when hiding left/right, card left when hiding top/bottom.
    pub along: i32,
}

/// Where a released card should dock, if anywhere.
///
/// `threshold` is in pixels. A gap at or inside it is close enough to dock
/// (negative means the card already sticks out past that edge). A corner
/// hides by priority: top, then a side, then the bottom. The other edge
/// only parks the card in that corner.
pub fn plan(card: RECT, work: RECT, outer: Edges, threshold: i32) -> Option<Placement> {
    let gaps = [
        (Side::Left, card.left - work.left),
        (Side::Right, work.right - card.right),
        (Side::Top, card.top - work.top),
        (Side::Bottom, work.bottom - card.bottom),
    ];
    let mut candidates: Vec<(Side, i32)> = gaps
        .into_iter()
        .filter(|(side, gap)| outer.allows(*side) && *gap <= threshold)
        .collect();

    if candidates.is_empty() {
        if intersects(card, work) {
            return None;
        }
        candidates = gaps.into_iter().filter(|(side, _)| outer.allows(*side)).collect();
        if candidates.is_empty() {
            candidates = gaps.to_vec();
        }
    }

    let edge = choose_edge(&candidates);
    let corner = candidates
        .iter()
        .copied()
        .filter(|(side, _)| perpendicular(*side, edge))
        .min_by_key(|(side, gap)| (rank(*side), *gap))
        .map(|(side, _)| side);

    let card_w = (card.right - card.left).max(1);
    let card_h = (card.bottom - card.top).max(1);
    let along = match edge {
        Side::Left | Side::Right => clamp_axis(card.top, card_h, work.top, work.bottom),
        Side::Top | Side::Bottom => clamp_axis(card.left, card_w, work.left, work.right),
    };
    Some(Placement { edge, corner, along })
}

/// Top wins over either side, and either side wins over the bottom.
fn rank(side: Side) -> i32 {
    match side {
        Side::Top => 0,
        Side::Left | Side::Right => 1,
        Side::Bottom => 2,
    }
}

fn choose_edge(candidates: &[(Side, i32)]) -> Side {
    candidates
        .iter()
        .min_by_key(|(side, gap)| (rank(*side), *gap))
        .map(|(side, _)| *side)
        .unwrap_or(Side::Right)
}

fn perpendicular(side: Side, edge: Side) -> bool {
    matches!(
        (side, edge),
        (Side::Left | Side::Right, Side::Top | Side::Bottom)
            | (Side::Top | Side::Bottom, Side::Left | Side::Right)
    )
}

fn clamp_axis(origin: i32, span: i32, start: i32, end: i32) -> i32 {
    let max_origin = (end - span).max(start);
    origin.clamp(start, max_origin)
}

pub fn intersects(a: RECT, b: RECT) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

pub struct Dock {
    pub monitor: HMONITOR,
    pub edge: Side,
    corner: Option<Side>,
    along: i32,
    slide: f32,
    slide_target: f32,
    from: Option<(i32, i32)>,
    pub t: f32,
    hover: f32,
    away: f32,
    /// Set once the pointer leaves the card after a drop, so the drag that
    /// caused the dock does not immediately count as a hover.
    hover_armed: bool,
    pub suspended: bool,
    pub shown: (i32, i32),
    pub work: RECT,
}

impl Dock {
    pub fn engage(
        monitor: HMONITOR,
        placement: Placement,
        from: (i32, i32),
        work: RECT,
    ) -> Self {
        Self {
            monitor,
            edge: placement.edge,
            corner: placement.corner,
            along: placement.along,
            slide: 1.0,
            slide_target: 1.0,
            from: Some(from),
            t: 0.0,
            hover: 0.0,
            away: 0.0,
            hover_armed: false,
            suspended: false,
            shown: from,
            work,
        }
    }

    pub fn tick(&mut self, dt: f32, hover: bool, card_w: i32, card_h: i32, peek: i32) {
        if self.suspended {
            return;
        }
        let dt = dt.min(0.1);
        let shown = self.origin(card_w, card_h, peek);
        self.shown = shown;
        if hover {
            self.away = 0.0;
            if self.hover_armed {
                self.hover += dt;
                if self.hover >= HOVER_SECS {
                    self.retarget(0.0, shown);
                }
            }
        } else {
            self.hover = 0.0;
            self.hover_armed = true;
            self.away += dt;
            if self.away >= AWAY_SECS {
                self.retarget(1.0, shown);
            }
        }
        if self.t < 1.0 {
            self.t = (self.t + dt / SLIDE_SECS).min(1.0);
            if self.t >= 1.0 {
                self.slide = self.slide_target;
                self.from = None;
            }
        }
    }

    pub fn retarget(&mut self, target: f32, from: (i32, i32)) {
        if (self.slide_target - target).abs() < 0.01 {
            return;
        }
        self.from = Some(from);
        self.t = 0.0;
        self.slide_target = target;
        self.shown = from;
        if target < 0.5 {
            self.hover_armed = true;
            self.away = 0.0;
        }
    }

    pub fn origin(&self, card_w: i32, card_h: i32, peek: i32) -> (i32, i32) {
        let slide = if self.t >= 1.0 { self.slide } else { self.slide_target };
        let end = self.parked(slide, card_w, card_h, peek);
        match self.from {
            Some(from) if self.t < 1.0 => {
                // A release past the peek springs back to it and overshoots
                // a little, instead of staying outside the screen.
                let hidden = self.parked(1.0, card_w, card_h, peek);
                let e = if self.slide_target >= 0.5 && beyond_peek(self.edge, from, hidden) {
                    bounce_back(self.t)
                } else {
                    smooth(self.t)
                };
                (lerp_i(from.0, end.0, e), lerp_i(from.1, end.1, e))
            }
            _ => end,
        }
    }

    pub fn is_tucked(&self, card_w: i32, card_h: i32) -> bool {
        if self.suspended {
            return false;
        }
        let (x, y) = self.shown;
        let visible = match self.edge {
            Side::Right => self.work.right - x,
            Side::Left => x + card_w - self.work.left,
            Side::Bottom => self.work.bottom - y,
            Side::Top => y + card_h - self.work.top,
        };
        let span = match self.edge {
            Side::Left | Side::Right => card_w,
            Side::Top | Side::Bottom => card_h,
        };
        visible * 2 < span.max(1)
    }

    fn parked(&self, slide: f32, card_w: i32, card_h: i32, peek: i32) -> (i32, i32) {
        let work = self.work;
        let slide = slide.clamp(0.0, 1.0);
        let x = match self.edge {
            Side::Left => {
                let peek = peek.clamp(1, card_w.max(1));
                lerp_i(work.left, work.left + peek - card_w, slide)
            }
            Side::Right => {
                let peek = peek.clamp(1, card_w.max(1));
                lerp_i(work.right - card_w, work.right - peek, slide)
            }
            _ => self.free_x(card_w),
        };
        let y = match self.edge {
            Side::Top => {
                let peek = peek.clamp(1, card_h.max(1));
                lerp_i(work.top, work.top + peek - card_h, slide)
            }
            Side::Bottom => {
                let peek = peek.clamp(1, card_h.max(1));
                lerp_i(work.bottom - card_h, work.bottom - peek, slide)
            }
            _ => self.free_y(card_h),
        };
        (x, y)
    }

    fn free_x(&self, card_w: i32) -> i32 {
        let max_x = (self.work.right - card_w).max(self.work.left);
        match self.corner {
            Some(Side::Left) => self.work.left,
            Some(Side::Right) => max_x,
            _ => self.along.clamp(self.work.left, max_x),
        }
    }

    fn free_y(&self, card_h: i32) -> i32 {
        let max_y = (self.work.bottom - card_h).max(self.work.top);
        match self.corner {
            Some(Side::Top) => self.work.top,
            Some(Side::Bottom) => max_y,
            _ => self.along.clamp(self.work.top, max_y),
        }
    }
}

fn lerp_i(a: i32, b: i32, t: f32) -> i32 {
    (a as f32 + (b - a) as f32 * t).round() as i32
}

fn beyond_peek(edge: Side, from: (i32, i32), hidden: (i32, i32)) -> bool {
    match edge {
        Side::Right => from.0 > hidden.0,
        Side::Left => from.0 < hidden.0,
        Side::Bottom => from.1 > hidden.1,
        Side::Top => from.1 < hidden.1,
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Ease out with a small overshoot, the way a released window springs back.
fn bounce_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u * u * u + c1 * u * u
}

pub fn monitor_from_point(point: POINT) -> HMONITOR {
    unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) }
}

pub fn monitor_info(monitor: HMONITOR) -> Option<MONITORINFO> {
    if monitor.is_invalid() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    unsafe {
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            Some(info)
        } else {
            None
        }
    }
}

/// Edges of `monitor` that do not touch another monitor.
pub fn outer_edges(monitor: HMONITOR) -> Edges {
    let Some(mine) = monitor_info(monitor) else {
        return Edges::all();
    };
    let mine = mine.rcMonitor;
    let mut edges = Edges::all();
    for (other, rect) in monitors() {
        if other == monitor {
            continue;
        }
        let overlap_x = rect.left < mine.right - 32 && rect.right > mine.left + 32;
        let overlap_y = rect.top < mine.bottom - 32 && rect.bottom > mine.top + 32;
        if overlap_y && (rect.right - mine.left).abs() <= 16 {
            edges.left = false;
        }
        if overlap_y && (rect.left - mine.right).abs() <= 16 {
            edges.right = false;
        }
        if overlap_x && (rect.bottom - mine.top).abs() <= 16 {
            edges.top = false;
        }
        if overlap_x && (rect.top - mine.bottom).abs() <= 16 {
            edges.bottom = false;
        }
    }
    edges
}

fn monitors() -> Vec<(HMONITOR, RECT)> {
    let mut list = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor),
            LPARAM(&mut list as *mut Vec<(HMONITOR, RECT)> as isize),
        );
    }
    list
}

unsafe extern "system" fn enum_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let list = &mut *(data.0 as *mut Vec<(HMONITOR, RECT)>);
    if let Some(info) = monitor_info(monitor) {
        list.push((monitor, info.rcMonitor));
    }
    BOOL(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT { left, top, right, bottom }
    }

    fn place(card: RECT, outer: Edges) -> Option<Placement> {
        plan(card, rect(0, 0, 1000, 800), outer, 24)
    }

    #[test]
    fn stays_put_when_clear_of_every_edge() {
        let card = rect(200, 200, 500, 350);
        assert_eq!(place(card, Edges::all()), None);
    }

    #[test]
    fn hides_toward_the_one_near_edge() {
        let card = rect(680, 100, 980, 250);
        let placement = place(card, Edges::all()).unwrap();
        assert_eq!(placement.edge, Side::Right);
        assert_eq!(placement.corner, None);
        assert_eq!(placement.along, 100);
    }

    #[test]
    fn a_bottom_corner_hides_sideways() {
        // The bottom sticks out further, and the side still wins.
        let card = rect(680, 670, 980, 820);
        let placement = place(card, Edges::all()).unwrap();
        assert_eq!(placement.edge, Side::Right);
        assert_eq!(placement.corner, Some(Side::Bottom));
    }

    #[test]
    fn a_top_corner_hides_upward() {
        // The right sticks out further, and the top still wins.
        let card = rect(730, 10, 1030, 160);
        let placement = place(card, Edges::all()).unwrap();
        assert_eq!(placement.edge, Side::Top);
        assert_eq!(placement.corner, Some(Side::Right));
    }

    #[test]
    fn a_shared_edge_is_not_a_dock() {
        let card = rect(900, 100, 1100, 250);
        let outer = Edges { right: false, ..Edges::all() };
        assert_eq!(place(card, outer), None);
    }

    #[test]
    fn a_card_left_outside_comes_back_to_that_edge() {
        let card = rect(1100, 100, 1400, 250);
        let placement = place(card, Edges::all()).unwrap();
        assert_eq!(placement.edge, Side::Right);
        assert_eq!(placement.along, 100);
    }

    #[test]
    fn along_stays_inside_the_work_area() {
        // The top is a shared edge, so sticking out there does not dock upward.
        let card = rect(700, -40, 980, 110);
        let outer = Edges { top: false, ..Edges::all() };
        let placement = place(card, outer).unwrap();
        assert_eq!(placement.edge, Side::Right);
        assert_eq!(placement.along, 0);
    }

    #[test]
    fn slide_ends_on_the_peek() {
        let work = rect(0, 0, 1000, 800);
        let from = (900, 120);
        let placement = Placement { edge: Side::Right, corner: None, along: 120 };
        let mut dock = Dock::engage(HMONITOR::default(), placement, from, work);
        let mut steps = 0;
        while dock.t < 1.0 && steps < 20 {
            dock.tick(0.05, false, 100, 150, 8);
            steps += 1;
        }
        assert_eq!(dock.origin(100, 150, 8), (1000 - 8, 120));
        assert!(dock.is_tucked(100, 150));
    }

    #[test]
    fn the_drop_itself_does_not_count_as_a_hover() {
        let work = rect(0, 0, 1000, 800);
        let placement = Placement { edge: Side::Right, corner: None, along: 120 };
        let mut dock = Dock::engage(HMONITOR::default(), placement, (900, 120), work);
        dock.tick(0.3, true, 100, 150, 8);
        assert!((dock.slide_target - 1.0).abs() < 0.01);
        dock.tick(0.05, false, 100, 150, 8);
        dock.tick(0.1, true, 100, 150, 8);
        dock.tick(0.1, true, 100, 150, 8);
        assert!((dock.slide_target - 0.0).abs() < 0.01);
    }

    #[test]
    fn a_corner_keeps_the_other_edge_flush() {
        let work = rect(0, 0, 1000, 800);
        let placement = Placement { edge: Side::Right, corner: Some(Side::Bottom), along: 400 };
        let mut dock = Dock::engage(HMONITOR::default(), placement, (880, 400), work);
        for _ in 0..8 {
            dock.tick(0.1, false, 120, 150, 8);
        }
        assert_eq!(dock.origin(120, 150, 8), (1000 - 8, 800 - 150));
    }

    #[test]
    fn a_drop_outside_springs_back_to_the_peek() {
        let work = rect(0, 0, 1000, 800);
        let from = (1200, 100);
        let placement = Placement { edge: Side::Right, corner: None, along: 100 };
        let mut dock = Dock::engage(HMONITOR::default(), placement, from, work);
        let peek_x = 1000 - 8;
        let mut innermost = from.0;
        for _ in 0..40 {
            dock.tick(0.02, false, 200, 150, 8);
            innermost = innermost.min(dock.origin(200, 150, 8).0);
        }
        assert!(innermost < peek_x);
        assert_eq!(dock.origin(200, 150, 8), (peek_x, 100));
    }
}
