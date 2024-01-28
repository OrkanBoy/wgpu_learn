use std::cmp::{min, max};

use crate::math::{self, Vector3};
use math::Vector2;

/// `points`: points to be wrapped by convex polygon; points also get sorted to avoid unintended allocation.
/// `prev_indices`: contains an index to the `points` slice if corresponding point is on convex hull.
/// (within graham_scan `indices_on_convex` used for each point on convex to refer to previous point on convex)
/// returns the number of points on convex hull, or effective len of prev_indices.
pub fn graham_scan(
    points: &mut [Vector2],
    indices_on_hull: &mut [usize],
) -> usize {
    let mut min_i = 0;
    for i in 1..points.len() {
        if points[i].y < points[min_i].y {
            min_i = i;
        }
    }

    let point_0 = points[min_i];
    points[min_i] = points[0];
    points[0] = point_0;

    points[1..].sort_unstable_by(|a, b| {
        // (b.x - point_0.x) / (b.y - point_0.y) < (a.x - point_0.x) / (a.y - point_0.y)
        let a_dy = a.y - point_0.y;
        let b_dy = b.y - point_0.y;
        let a_dx = a.x - point_0.x;
        let b_dx = b.x - point_0.x;

        let lhs = b_dx * a_dy;
        let rhs = a_dx * b_dy;
        // check sign
        if (a_dy >= 0.0 && b_dy >= 0.0)
        || (a_dy < 0.0 && b_dy < 0.0) {
            lhs.total_cmp(&rhs)
        } else {
            rhs.total_cmp(&lhs)   
        }
    });
    let last_i = points.len() - 1;

    indices_on_hull[0] = 0;
    indices_on_hull[1] = 1;
    let mut prev_indices_last_i = 1;

    let mut i = 2;
    let mut old_vec = points[2] - points[1];

    while i < last_i {
        let mut j = i;
        i += 1;
        let mut new_vec = points[i] - points[j];
        let mut turn = old_vec.wedge(new_vec);

        while turn.xy <= 0.0 {
            j = indices_on_hull[prev_indices_last_i];
            prev_indices_last_i -= 1;
            let prev_j = indices_on_hull[prev_indices_last_i];

            old_vec = points[j] - points[prev_j];
            new_vec = points[i] - points[j];
            turn = old_vec.wedge(new_vec);       
        }

        old_vec = new_vec;
        prev_indices_last_i += 1;
        indices_on_hull[prev_indices_last_i] = j;
    }

    prev_indices_last_i += 1;
    indices_on_hull[prev_indices_last_i] = last_i;

    prev_indices_last_i + 1
}

/// returns a Vec of the intersection of 2 polygons which is also a polygon
/// allocations: the Vec
/// algorithm used: https://www.cs.jhu.edu/~misha/Spring16/ORourke82.pdf
pub fn convex_intersect_alloc(
    convex_p: &[Vector2],
    convex_q: &[Vector2],
) -> Vec<Vector2> {
    let mut p_i = 1;
    let mut q_i = 1;

    let mut p = &convex_p[1];
    let mut q = &convex_q[1];
    let mut old_p = &convex_p[0];
    let mut old_q = &convex_q[0];

    let mut dp = *p - *old_p;
    let mut dq = *q - *old_q;

    let mut convex_r = vec![];
    let mut inside = b' ';

    for _ in 0..2 * (convex_p.len() + convex_q.len()) {
        let old_q_to_p = *p - *old_q;

        let dq_dp = dq.wedge(dp).xy;
        let p_in_dq_side = dq.wedge(old_q_to_p).xy > 0.0;

        let dold = *old_q - *old_p;
        let t = dq.wedge(dold).xy / dq_dp;
        let s = dp.wedge(dold).xy / dq_dp;

        if t >= 0.0 && t <= 1.0 && s >= 0.0 && s <= 1.0 {
            let r = *old_p + dp * t;
            if convex_r.len() != 0 && convex_r[0] == r {
                break;
            } else {
                convex_r.push(r);
            }

            inside = if p_in_dq_side {
                b'P'
            } else {
                b'Q'
            }
        }
        
        let old_p_to_q = *q - *old_p;
        let q_in_dp_side = dp.wedge(old_p_to_q).xy > 0.0;
        // ccw: counter clock wise
        let dq_dp_ccw = dq_dp > 0.0;

        if (dq_dp_ccw && p_in_dq_side) || (!dq_dp_ccw && !q_in_dp_side) {
            if inside == b'Q' {
                convex_r.push(*q);
            }
            q_i = (q_i + 1) % convex_q.len();
            old_q = q;
            q = &convex_q[q_i];
            dq = *q - *old_q;
        } else {
            if inside == b'P' {
                convex_r.push(*p);
            }
            p_i = (p_i + 1) % convex_p.len();
            old_p = p;
            p = &convex_p[p_i];
            dp = *p - *old_p;
        }
    }

    return convex_r;
} 

/// writes to the slice provided, it writes the intersection of 2 polygons which is also a polygon
/// returns the size of the resulting polygon
/// no allocations
/// algorithm used: https://www.cs.jhu.edu/~misha/Spring16/ORourke82.pdf
pub fn convex_intersect_no_alloc(
    convex_p: &[Vector2],
    convex_q: &[Vector2],
    convex_r: &mut [Vector2],
) -> usize {
    let mut convex_r_len = 0;
    let mut p_i = 1;
    let mut q_i = 1;

    let mut p = &convex_p[p_i];
    let mut q = &convex_q[q_i];
    let mut old_p = &convex_p[p_i - 1];
    let mut old_q = &convex_q[q_i - 1];

    let mut dp = *p - *old_p;
    let mut dq = *q - *old_q;

    let mut inside = b' ';

    for _ in 0..2 * (convex_p.len() + convex_q.len()) {
        let old_q_to_p = *p - *old_q;

        let dq_dp = dq.wedge(dp).xy;
        let p_in_dq_side = dq.wedge(old_q_to_p).xy > 0.0;

        let dold = *old_q - *old_p;
        // doesn't well follow degenrate cases possibly due to possible division by zero x/
        let t = dq.wedge(dold).xy / dq_dp;
        let s = dp.wedge(dold).xy / dq_dp;

        if t >= 0.0 && t <= 1.0 && s >= 0.0 && s <= 1.0 {
            let r = *old_p + dp * t;
            if convex_r_len != 0 && convex_r[0] == r {
                break;
            } else {
                convex_r[convex_r_len] = r;
                convex_r_len += 1;
            }

            inside = if p_in_dq_side {
                b'P'
            } else {
                b'Q'
            }
        }
        
        let old_p_to_q = *q - *old_p;
        let q_in_dp_side = dp.wedge(old_p_to_q).xy > 0.0;
        // ccw: counter clock wise
        let dq_dp_ccw = dq_dp > 0.0;

        if (dq_dp_ccw && p_in_dq_side) || (!dq_dp_ccw && !q_in_dp_side) {
            if inside == b'Q' {
                convex_r[convex_r_len] = *q;
                convex_r_len += 1;
            }
            q_i = (q_i + 1) % convex_q.len();
            old_q = q;
            q = &convex_q[q_i];
            dq = *q - *old_q;
        } else {
            if inside == b'P' {
                convex_r[convex_r_len] = *p;
                convex_r_len += 1;
            }
            p_i = (p_i + 1) % convex_p.len();
            old_p = p;
            p = &convex_p[p_i];
            dp = *p - *old_p;
        }
    }

    return convex_r_len;
} 

pub struct Rect {
    pub max: Vector2,
    pub min: Vector2,
}

impl Rect {
    /// assumes points form rect with non-zero dimensions
    pub fn from_points(points: &[Vector2]) -> Rect {
        let mut rect = Rect {
            max: points[0],
            min: points[0],
        };
        for point in points.iter() {
            if point.x > rect.max.x {
                rect.max.x = point.x;
            } else if point.x < rect.min.x {
                rect.min.x = point.x;
            }
    
            if point.y > rect.max.y {
                rect.max.y = point.y;
            } else if point.y < rect.min.y {
                rect.min.y = point.y;
            }
        }
        rect
    }

    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        if self.max.x <= other.min.x || self.max.y <= other.min.y
        || other.max.x <= self.min.x || other.max.y <= self.min.y {
            None
        } else {
            Some(Rect{
                max: Vector2::new(
                    f32::min(self.max.x, other.max.x),
                    f32::min(self.max.y, other.max.y),
                ),
                min: Vector2::new(
                    f32::max(self.min.x, other.min.x),
                    f32::max(self.min.y, other.min.y),
                ),
            })
        }
    }

    #[inline(always)]
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    #[inline(always)]
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }
}