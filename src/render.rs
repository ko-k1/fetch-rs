//! Layout and per-cell glyph collapse, ported from fetch.c.

use crate::ansi::visible_width;
use crate::shading::{QUADRANT_GLYPHS, Shading, sextant_glyph};

pub const MAX_HEIGHT: i32 = 200;
pub const ANIM_WIDTH: i32 = 60;
pub const MAX_SUB_ROWS: i32 = 3;
pub const MAX_SUB_COLS: i32 = 2;
pub const SUB_H: i32 = MAX_HEIGHT * MAX_SUB_ROWS; // 600
pub const SUB_W: i32 = ANIM_WIDTH * MAX_SUB_COLS; // 120
pub const GAP: i32 = 2;
pub const K2_CONST: f32 = 5.5;

pub struct Buffers {
    pub zbuf: Vec<f32>,     // SUB_H * SUB_W
    pub lumbuf: Vec<f32>,   // SUB_H * SUB_W
    pub colorbuf: Vec<i32>, // SUB_H * SUB_W
}

impl Buffers {
    pub fn new() -> Self {
        let n = (SUB_H * SUB_W) as usize;
        Self {
            zbuf: vec![0.0; n],
            lumbuf: vec![0.0; n],
            colorbuf: vec![0; n],
        }
    }

    pub fn clear(&mut self, rows: i32, sub_rows: i32) {
        let n = (rows * sub_rows * SUB_W) as usize;
        self.zbuf[..n].fill(0.0);
        self.lumbuf[..n].fill(0.0);
        self.colorbuf[..n].fill(0);
    }
}

pub struct Layout {
    pub render_height: i32,
    pub logo_height: i32,
    pub anim_width: i32,
    pub layout_stacked: bool,
    pub info_clip_cols: i32,
    pub stacked_info_rows: i32,
    pub term_cols: i32,
    pub term_rows: i32,
}

impl Layout {
    pub fn new() -> Self {
        Self {
            render_height: 36,
            logo_height: 36,
            anim_width: ANIM_WIDTH,
            layout_stacked: false,
            info_clip_cols: -1,
            stacked_info_rows: 0,
            term_cols: 0,
            term_rows: 0,
        }
    }
}

/// Sizing + layout, shared by startup and resize.
pub fn apply_layout(
    layout: &mut Layout,
    show_info: bool,
    fetch_line_count: i32,
    size_scale: f32,
    config_height: i32,
    fetch_lines: &[String],
) {
    let mut rows = layout.term_rows;
    let cols = layout.term_cols;
    if rows > 1 {
        rows -= 1; // leave 1 row margin
    }

    let mut ideal = (36.0 * size_scale) as i32;
    if config_height > 0 {
        ideal = config_height;
    } else if show_info && fetch_line_count > 0 {
        let info_height = fetch_line_count + 2;
        if ideal < info_height {
            ideal = info_height;
        }
    }
    if ideal < 20 {
        ideal = 20;
    }
    if ideal > MAX_HEIGHT {
        ideal = MAX_HEIGHT;
    }

    let mut render_height = ideal;
    let mut anim_width = ANIM_WIDTH;
    let mut layout_stacked = false;
    let mut info_clip_cols = -1;
    let mut stacked_info_rows = 0;

    if cols <= 0 {
        // no width info (not a tty): cap height only
        if rows > 0 && render_height > rows {
            render_height = rows;
        }
        layout.render_height = render_height;
        layout.logo_height = render_height;
        return;
    }

    let mut info_cols = 0;
    if show_info {
        for i in 0..fetch_line_count {
            let w = visible_width(&fetch_lines[i as usize]);
            if w > info_cols {
                info_cols = w;
            }
        }
    }

    if show_info && cols < ANIM_WIDTH + GAP + info_cols as i32 {
        anim_width = cols - GAP - info_cols as i32;
        if anim_width < 20 {
            layout_stacked = true;
            anim_width = cols.min(ANIM_WIDTH);
        }
    } else if anim_width > cols {
        anim_width = cols;
    }

    let wcap = anim_width * 3 / 5;

    if layout_stacked {
        if render_height > wcap {
            render_height = wcap;
        }
        stacked_info_rows = fetch_line_count;
        if rows > 0 {
            let budget = rows - fetch_line_count - 1; // 1 spacer row
            if budget < render_height {
                render_height = budget;
            }
            if render_height < 6 {
                render_height = 0; // too small: show info only
            }
            let info_budget = rows
                - if render_height > 0 {
                    render_height + 1
                } else {
                    0
                };
            if stacked_info_rows > info_budget {
                stacked_info_rows = info_budget.max(0);
            }
        }
        layout.logo_height = render_height;
        info_clip_cols = cols;
    } else {
        if show_info {
            info_clip_cols = cols - anim_width - GAP;
        }
        if rows > 0 && render_height > rows {
            render_height = rows;
        }
        layout.logo_height = if render_height > wcap {
            wcap
        } else {
            render_height
        };
        if !show_info {
            render_height = layout.logo_height;
        }
    }

    layout.render_height = render_height;
    layout.anim_width = anim_width;
    layout.layout_stacked = layout_stacked;
    layout.info_clip_cols = info_clip_cols;
    layout.stacked_info_rows = stacked_info_rows;
}

/// Collapse one cell's sub-samples into a glyph. Returns None for an empty cell.
pub fn cell_glyph<'a>(
    row: i32,
    col: i32,
    smax: i32,
    buffers: &Buffers,
    sub_rows: i32,
    sub_cols: i32,
    shading: &'a Shading,
    color_out: &mut i32,
) -> Option<&'a str> {
    let x0 = col * sub_cols;
    let y0 = row * sub_rows;
    let total = (sub_rows * sub_cols) as usize;
    let mut mask: u32 = 0;
    let mut bit = 0u32;
    let mut n = 0i32;
    let mut lsum = 0.0f32;
    let mut best = 0.0f32;

    for sr in 0..sub_rows {
        for sc in 0..sub_cols {
            let z = buffers.zbuf[((y0 + sr) * SUB_W + (x0 + sc)) as usize];
            if z <= 0.0 {
                continue;
            }
            mask |= 1 << bit;
            lsum += buffers.lumbuf[((y0 + sr) * SUB_W + (x0 + sc)) as usize];
            n += 1;
            if z > best {
                best = z;
                *color_out = buffers.colorbuf[((y0 + sr) * SUB_W + (x0 + sc)) as usize];
            }
            bit += 1;
        }
    }

    if n == 0 {
        return None;
    }

    let coverage = n as f32 / total as f32;
    let ink = lsum / n as f32 * coverage;
    let mut ci = (ink * smax as f32 + 0.5) as i32;
    if ci < 0 {
        ci = 0;
    }
    if ci > smax {
        ci = smax;
    }
    let full_mask = (1u32 << total) - 1;
    if mask != full_mask
        && (coverage - ink).abs() <= (((ci + 1) as f32 / shading.chars.len() as f32) - ink).abs()
    {
        if sub_rows == 3 {
            return sextant_glyph(mask);
        }
        return Some(QUADRANT_GLYPHS[mask as usize]);
    }
    Some(shading.ramp(ci as usize))
}
