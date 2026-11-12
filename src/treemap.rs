/// Axis-aligned rectangle in cell coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub fn area(self) -> u32 {
        u32::from(self.w) * u32::from(self.h)
    }
}

/// One laid-out treemap tile.
#[derive(Debug, Clone)]
pub struct Tile {
    pub index: usize,
    pub rect: Rect,
}

/// Squarified treemap layout (Bruls et al.).
pub fn layout(sizes: &[u64], bounds: Rect) -> Vec<Tile> {
    if sizes.is_empty() || bounds.w == 0 || bounds.h == 0 {
        return Vec::new();
    }

    let total: u64 = sizes.iter().sum();
    if total == 0 {
        return Vec::new();
    }

    let mut items: Vec<(usize, f64)> = sizes
        .iter()
        .enumerate()
        .filter(|(_, &s)| s > 0)
        .map(|(i, &s)| (i, s as f64 / total as f64 * bounds.area() as f64))
        .collect();

    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut tiles = Vec::with_capacity(items.len());
    squarify(&items, bounds, &mut tiles);
    tiles
}

fn squarify(items: &[(usize, f64)], mut free: Rect, out: &mut Vec<Tile>) {
    if items.is_empty() || free.w == 0 || free.h == 0 {
        return;
    }

    let mut row: Vec<(usize, f64)> = Vec::new();
    let mut remaining = items;

    while !remaining.is_empty() {
        let (idx, area) = remaining[0];
        let candidate = {
            let mut next = row.clone();
            next.push((idx, area));
            next
        };

        if row.is_empty() || worst(&row, free) >= worst(&candidate, free) {
            row = candidate;
            remaining = &remaining[1..];
            if remaining.is_empty() {
                layout_row(&row, &mut free, out);
            }
        } else {
            layout_row(&row, &mut free, out);
            row.clear();
            if free.w == 0 || free.h == 0 {
                break;
            }
        }
    }
}

fn short_side(r: Rect) -> f64 {
    f64::from(r.w.min(r.h))
}

fn worst(row: &[(usize, f64)], free: Rect) -> f64 {
    if row.is_empty() {
        return f64::INFINITY;
    }
    let s = short_side(free);
    if s == 0.0 {
        return f64::INFINITY;
    }
    let sum: f64 = row.iter().map(|(_, a)| *a).sum();
    if sum <= 0.0 {
        return f64::INFINITY;
    }
    let mut max_r = 0.0f64;
    let mut min_r = f64::INFINITY;
    for (_, a) in row {
        max_r = max_r.max(*a);
        min_r = min_r.min(*a);
    }
    ((s * s * max_r) / (sum * sum)).max((sum * sum) / (s * s * min_r))
}

fn layout_row(row: &[(usize, f64)], free: &mut Rect, out: &mut Vec<Tile>) {
    if row.is_empty() || free.w == 0 || free.h == 0 {
        return;
    }
    let sum: f64 = row.iter().map(|(_, a)| *a).sum();
    let horizontal = free.w >= free.h;

    if horizontal {
        let mut h = (sum / f64::from(free.w)).round() as u16;
        h = h.clamp(1, free.h);
        let mut x = free.x;
        let mut left = sum;
        for (i, (idx, area)) in row.iter().enumerate() {
            let w = if i + 1 == row.len() {
                free.x + free.w - x
            } else {
                let share = (*area / left) * f64::from(free.w - (x - free.x));
                left -= area;
                let w = share.round().max(1.0) as u16;
                w.min(free.x + free.w - x)
            };
            if w == 0 {
                continue;
            }
            out.push(Tile {
                index: *idx,
                rect: Rect { x, y: free.y, w, h },
            });
            x = x.saturating_add(w);
        }
        free.y = free.y.saturating_add(h);
        free.h = free.h.saturating_sub(h);
    } else {
        let mut w = (sum / f64::from(free.h)).round() as u16;
        w = w.clamp(1, free.w);
        let mut y = free.y;
        let mut left = sum;
        for (i, (idx, area)) in row.iter().enumerate() {
            let h = if i + 1 == row.len() {
                free.y + free.h - y
            } else {
                let share = (*area / left) * f64::from(free.h - (y - free.y));
                left -= area;
                let h = share.round().max(1.0) as u16;
                h.min(free.y + free.h - y)
            };
            if h == 0 {
                continue;
            }
            out.push(Tile {
                index: *idx,
                rect: Rect { x: free.x, y, w, h },
            });
            y = y.saturating_add(h);
        }
        free.x = free.x.saturating_add(w);
        free.w = free.w.saturating_sub(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_fills_nonzero_sizes() {
        let sizes = [50u64, 30, 20];
        let tiles = layout(
            &sizes,
            Rect {
                x: 0,
                y: 0,
                w: 40,
                h: 20,
            },
        );
        assert_eq!(tiles.len(), 3);
        let area: u32 = tiles.iter().map(|t| t.rect.area()).sum();
        assert!(area > 0);
        assert!(area <= 40 * 20);
    }
}
