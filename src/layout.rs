//! Split-screen geometry: pane rectangles, resizable dividers, adding/removing panes.

use serde::{Deserialize, Serialize};

pub const GAP: f64 = 8.0;
const MIN_W: f64 = 0.14;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct R {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    H,
    V,
    Grid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Split {
    pub ids: Vec<usize>,
    pub mode: Mode,
    pub cw: Vec<f64>,
    pub rw: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Div {
    pub col: bool,
    pub idx: usize,
    pub r: R,
}

fn equal(n: usize) -> Vec<f64> {
    vec![1.0 / n as f64; n]
}

fn spans(start: f64, total: f64, w: &[f64]) -> Vec<(f64, f64)> {
    let n = w.len();
    let avail = (total - GAP * (n as f64 - 1.0)).max(1.0);
    let sum: f64 = w.iter().sum::<f64>().max(0.0001);
    let mut p = start;
    w.iter()
        .map(|f| {
            let s = f / sum * avail;
            let o = (p, s);
            p += s + GAP;
            o
        })
        .collect()
}

impl Split {
    pub fn pair(a: usize, b: usize, edge: &str, _existing_first: bool) -> Split {
        let (ids, mode) = match edge {
            "left" => (vec![b, a], Mode::H),
            "top" => (vec![b, a], Mode::V),
            "bottom" => (vec![a, b], Mode::V),
            _ => (vec![a, b], Mode::H),
        };
        let mut s = Split {
            ids,
            mode,
            cw: vec![],
            rw: vec![],
        };
        s.reset();
        s
    }

    pub fn reset(&mut self) {
        let n = self.ids.len();
        match self.mode {
            Mode::H => {
                self.cw = equal(n);
                self.rw = vec![1.0];
            }
            Mode::V => {
                self.cw = vec![1.0];
                self.rw = equal(n);
            }
            Mode::Grid => {
                self.cw = equal(2);
                self.rw = equal(2);
            }
        }
    }

    /// Add a pane at an edge. Returns false when already at 4 panes or already present.
    pub fn add(&mut self, id: usize, edge: &str) -> bool {
        if self.ids.len() >= 4 || self.ids.contains(&id) {
            return false;
        }
        let front = matches!(edge, "left" | "top");
        let horizontal = matches!(edge, "left" | "right");
        let fits = (self.mode == Mode::H && horizontal) || (self.mode == Mode::V && !horizontal);
        if fits {
            if front {
                self.ids.insert(0, id);
            } else {
                self.ids.push(id);
            }
        } else {
            self.ids.push(id);
            self.mode = Mode::Grid;
        }
        if self.ids.len() == 4 && self.mode != Mode::Grid {
            self.mode = Mode::Grid;
        }
        self.reset();
        true
    }

    /// Remove a pane. Returns false when the split dissolves (fewer than 2 panes).
    pub fn remove(&mut self, id: usize) -> bool {
        self.ids.retain(|x| *x != id);
        if self.ids.len() < 2 {
            return false;
        }
        if self.mode == Mode::Grid && self.ids.len() == 2 {
            self.mode = Mode::H;
        }
        self.reset();
        true
    }

    pub fn panes(&self, r: R) -> Vec<R> {
        let n = self.ids.len();
        match self.mode {
            Mode::H => spans(r.x, r.w, &self.cw)
                .into_iter()
                .map(|(x, w)| R { x, y: r.y, w, h: r.h })
                .collect(),
            Mode::V => spans(r.y, r.h, &self.rw)
                .into_iter()
                .map(|(y, h)| R { x: r.x, y, w: r.w, h })
                .collect(),
            Mode::Grid => {
                let cs = spans(r.x, r.w, &self.cw);
                let rs = spans(r.y, r.h, &self.rw);
                if n == 3 {
                    vec![
                        R {
                            x: cs[0].0,
                            y: r.y,
                            w: cs[0].1,
                            h: r.h,
                        },
                        R {
                            x: cs[1].0,
                            y: rs[0].0,
                            w: cs[1].1,
                            h: rs[0].1,
                        },
                        R {
                            x: cs[1].0,
                            y: rs[1].0,
                            w: cs[1].1,
                            h: rs[1].1,
                        },
                    ]
                } else {
                    (0..n)
                        .map(|i| R {
                            x: cs[i % 2].0,
                            y: rs[i / 2].0,
                            w: cs[i % 2].1,
                            h: rs[i / 2].1,
                        })
                        .collect()
                }
            }
        }
    }

    pub fn dividers(&self, r: R) -> Vec<Div> {
        let mut out = vec![];
        let cs = spans(r.x, r.w, &self.cw);
        let rs = spans(r.y, r.h, &self.rw);
        match self.mode {
            Mode::H => {
                for (i, c) in cs.iter().enumerate().take(cs.len() - 1) {
                    out.push(Div {
                        col: true,
                        idx: i,
                        r: R {
                            x: c.0 + c.1,
                            y: r.y,
                            w: GAP,
                            h: r.h,
                        },
                    });
                }
            }
            Mode::V => {
                for (i, rw) in rs.iter().enumerate().take(rs.len() - 1) {
                    out.push(Div {
                        col: false,
                        idx: i,
                        r: R {
                            x: r.x,
                            y: rw.0 + rw.1,
                            w: r.w,
                            h: GAP,
                        },
                    });
                }
            }
            Mode::Grid => {
                out.push(Div {
                    col: true,
                    idx: 0,
                    r: R {
                        x: cs[0].0 + cs[0].1,
                        y: r.y,
                        w: GAP,
                        h: r.h,
                    },
                });
                let (x, w) = if self.ids.len() == 3 { (cs[1].0, cs[1].1) } else { (r.x, r.w) };
                out.push(Div {
                    col: false,
                    idx: 0,
                    r: R {
                        x,
                        y: rs[0].0 + rs[0].1,
                        w,
                        h: GAP,
                    },
                });
            }
        }
        out
    }

    /// Move divider `idx` so it sits at pointer position `pos` (window coordinates).
    pub fn drag(&mut self, r: R, col: bool, idx: usize, pos: f64) {
        let (origin, total, w) = if col { (r.x, r.w, &mut self.cw) } else { (r.y, r.h, &mut self.rw) };
        if idx + 1 >= w.len() {
            return;
        }
        let sum: f64 = w.iter().sum();
        let before: f64 = w[..idx].iter().sum::<f64>() / sum;
        let pair = (w[idx] + w[idx + 1]) / sum;
        let frac = (pos - origin) / total.max(1.0);
        let a = (frac - before).clamp(MIN_W, (pair - MIN_W).max(MIN_W));
        for x in w.iter_mut() {
            *x /= sum;
        }
        w[idx] = a;
        w[idx + 1] = pair - a;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const R0: R = R {
        x: 260.0,
        y: 94.0,
        w: 1000.0,
        h: 600.0,
    };

    fn inside(p: &R) -> bool {
        p.x >= R0.x - 0.01
            && p.y >= R0.y - 0.01
            && p.x + p.w <= R0.x + R0.w + 0.01
            && p.y + p.h <= R0.y + R0.h + 0.01
            && p.w > 10.0
            && p.h > 10.0
    }

    #[test]
    fn pair_and_grow() {
        let mut s = Split::pair(1, 2, "right", true);
        assert_eq!(s.ids, vec![1, 2]);
        assert!(s.panes(R0).iter().all(inside));
        assert!(s.add(3, "right"));
        assert_eq!(s.mode, Mode::H);
        assert!(s.add(4, "bottom"));
        assert_eq!(s.mode, Mode::Grid);
        assert_eq!(s.panes(R0).len(), 4);
        assert!(s.panes(R0).iter().all(inside));
        assert!(!s.add(5, "left"));
    }

    #[test]
    fn grid_three() {
        let mut s = Split::pair(1, 2, "right", true);
        assert!(s.add(3, "bottom"));
        assert_eq!(s.mode, Mode::Grid);
        let p = s.panes(R0);
        assert_eq!(p.len(), 3);
        assert!(p.iter().all(inside));
        assert!((p[0].h - R0.h).abs() < 0.01);
    }

    #[test]
    fn drag_and_remove() {
        let mut s = Split::pair(1, 2, "right", true);
        s.drag(R0, true, 0, R0.x + 800.0);
        let p = s.panes(R0);
        assert!(p[0].w > 700.0 && p[0].w < 820.0);
        s.drag(R0, true, 0, R0.x + 5.0);
        assert!(s.panes(R0)[0].w > 100.0);
        assert!(s.add(3, "right"));
        assert!(s.remove(3));
        assert!(!s.remove(2));
    }

    #[test]
    fn top_left_order() {
        let s = Split::pair(1, 2, "left", true);
        assert_eq!(s.ids, vec![2, 1]);
        let s = Split::pair(1, 2, "top", true);
        assert_eq!((s.ids.clone(), s.mode), (vec![2, 1], Mode::V));
    }
}
