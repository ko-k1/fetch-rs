//! 3D point-cloud generation for the rotating logo, ported from fetch.c.

use crate::logo::Logo;
use crate::shading::char_weight_utf8;

const MAX_POINTS: usize = 200_000;

pub struct Points {
    pub px: Vec<f32>,
    pub py: Vec<f32>,
    pub pz: Vec<f32>,
    pub nx: Vec<f32>,
    pub ny: Vec<f32>,
    pub nz: Vec<f32>,
    pub pcolor: Vec<i32>,
}

pub fn build_points(
    logo: &Logo,
    size_scale: f32,
    config_depth: f32,
    depth_user_set: bool,
    sub_rows: i32,
) -> Points {
    let logo_rows = logo.rows.len() as i32;
    let logo_cols = logo.cols as i32;

    let sx = 0.07f32;
    let sy = 0.14f32;
    let cx = (logo_cols - 1) as f32 * 0.5;
    let cy = (logo_rows - 1) as f32 * 0.5;

    let mut z_layers = (6.0 * size_scale) as i32;
    if z_layers < 6 {
        z_layers = 6;
    }

    // hmap, normals
    let mut hmap: Vec<f32> = vec![0.0; (logo_rows * logo_cols) as usize];
    let mut gnx: Vec<f32> = vec![0.0; (logo_rows * logo_cols) as usize];
    let mut gny: Vec<f32> = vec![0.0; (logo_rows * logo_cols) as usize];
    let mut gnz: Vec<f32> = vec![0.0; (logo_rows * logo_cols) as usize];

    let idx = |r: i32, c: i32| (r * logo_cols + c) as usize;

    for r in 0..logo_rows {
        for c in 0..logo_cols {
            let w = if (c as usize) < logo.cells[r as usize].len() {
                char_weight_utf8(&logo.cells[r as usize][c as usize].0)
            } else {
                0.0
            };
            hmap[idx(r, c)] = w;
        }
    }

    // Auto-scale depth when not set explicitly
    let mut config_depth = config_depth;
    if !depth_user_set {
        let mut sum = 0.0f32;
        let mut sum2 = 0.0f32;
        let mut n = 0i32;
        for r in 0..logo_rows {
            for c in 0..logo_cols {
                let h = hmap[idx(r, c)];
                if h > 0.0 {
                    sum += h;
                    sum2 += h * h;
                    n += 1;
                }
            }
        }
        if n > 0 {
            let mean = sum / n as f32;
            let variance = sum2 / n as f32 - mean * mean;
            let stddev = if variance > 0.0 { variance.sqrt() } else { 0.0 };
            if stddev < 0.25 {
                let boost = 1.0 + 2.0 * (0.25 - stddev) / 0.25;
                config_depth *= boost;
            }
        }
    }

    let zmax = 0.18 * config_depth;

    for r in 0..logo_rows {
        for c in 0..logo_cols {
            let h = hmap[idx(r, c)];
            if h <= 0.0 {
                gnx[idx(r, c)] = 0.0;
                gny[idx(r, c)] = 0.0;
                gnz[idx(r, c)] = 1.0;
                continue;
            }
            let dhdx = if c > 0 && c < logo_cols - 1 {
                (hmap[idx(r, c + 1)] - hmap[idx(r, c - 1)]) * 0.5
            } else if c == 0 && logo_cols > 1 {
                hmap[idx(r, c + 1)] - hmap[idx(r, c)]
            } else if c > 0 {
                hmap[idx(r, c)] - hmap[idx(r, c - 1)]
            } else {
                0.0
            };
            let dhdy = if r > 0 && r < logo_rows - 1 {
                (hmap[idx(r + 1, c)] - hmap[idx(r - 1, c)]) * 0.5
            } else if r == 0 && logo_rows > 1 {
                hmap[idx(r + 1, c)] - hmap[idx(r, c)]
            } else if r > 0 {
                hmap[idx(r, c)] - hmap[idx(r - 1, c)]
            } else {
                0.0
            };

            let dhdx = dhdx / sx;
            let dhdy = dhdy / sy;

            let nnx = -dhdx;
            let nny = dhdy;
            let nnz = 1.0f32;
            let l = (nnx * nnx + nny * nny + nnz * nnz).sqrt();
            gnx[idx(r, c)] = nnx / l;
            gny[idx(r, c)] = nny / l;
            gnz[idx(r, c)] = nnz / l;
        }
    }

    let subdiv = ((size_scale * sub_rows as f32) as i32).max(sub_rows as i32);

    let mut px = Vec::with_capacity(MAX_POINTS);
    let mut py = Vec::with_capacity(MAX_POINTS);
    let mut pz = Vec::with_capacity(MAX_POINTS);
    let mut nx = Vec::with_capacity(MAX_POINTS);
    let mut ny = Vec::with_capacity(MAX_POINTS);
    let mut nz = Vec::with_capacity(MAX_POINTS);
    let mut pcolor = Vec::with_capacity(MAX_POINTS);

    for row in 0..logo_rows {
        for col in 0..logo_cols {
            let h = hmap[idx(row, col)];
            if h <= 0.0 {
                continue;
            }

            for sr in 0..subdiv {
                for sc in 0..subdiv {
                    let frow = row as f32 + sr as f32 / subdiv as f32;
                    let fcol = col as f32 + sc as f32 / subdiv as f32;

                    let mut ih = h;
                    if sr > 0 || sc > 0 {
                        let fr = sr as f32 / subdiv as f32;
                        let fc = sc as f32 / subdiv as f32;
                        let mut nr = if sr > 0 { row + 1 } else { row };
                        let mut nc = if sc > 0 { col + 1 } else { col };
                        if nr >= logo_rows {
                            nr = logo_rows - 1;
                        }
                        if nc >= logo_cols {
                            nc = logo_cols - 1;
                        }
                        let h00 = hmap[idx(row, col)];
                        let h10 = hmap[idx(nr, col)];
                        let h01 = hmap[idx(row, nc)];
                        let h11 = hmap[idx(nr, nc)];
                        ih = h00 * (1.0 - fr) * (1.0 - fc)
                            + h10 * fr * (1.0 - fc)
                            + h01 * (1.0 - fr) * fc
                            + h11 * fr * fc;
                        if ih <= 0.0 {
                            continue;
                        }
                    }

                    let ox = (fcol - cx) * sx;
                    let oy = (cy - frow) * sy;
                    let zr = ih * zmax;

                    // edge cells only get front + back layers
                    let mut is_edge = false;
                    for dr in -1..=1 {
                        if is_edge {
                            break;
                        }
                        for dc in -1..=1 {
                            if dr == 0 && dc == 0 {
                                continue;
                            }
                            let nr = row + dr;
                            let nc = col + dc;
                            let mut nh = 0.0;
                            if nr >= 0 && nr < logo_rows && nc >= 0 && nc < logo_cols {
                                nh = hmap[idx(nr, nc)];
                            }
                            if nh <= 0.0 {
                                is_edge = true;
                                break;
                            }
                        }
                    }
                    let layers = if is_edge || ih < 0.15 { 2 } else { z_layers };

                    for k in 0..layers {
                        if px.len() >= MAX_POINTS {
                            break;
                        }
                        let t = (k as f32 / (layers - 1) as f32) - 0.5;
                        px.push(ox);
                        py.push(oy);
                        pz.push(t * 2.0 * zr);
                        let color = if logo.has_ansi {
                            logo.cells[row as usize][col as usize].1
                        } else {
                            if k == 0 || k == layers - 1 { 1 } else { 0 }
                        };
                        pcolor.push(color);

                        if k == 0 {
                            nx.push(gnx[idx(row, col)]);
                            ny.push(gny[idx(row, col)]);
                            nz.push(-gnz[idx(row, col)]);
                        } else if k == layers - 1 {
                            nx.push(gnx[idx(row, col)]);
                            ny.push(gny[idx(row, col)]);
                            nz.push(gnz[idx(row, col)]);
                        } else {
                            let mut ex = 0.0f32;
                            let mut ey = 0.0f32;
                            for dr in -1..=1 {
                                for dc in -1..=1 {
                                    if dr == 0 && dc == 0 {
                                        continue;
                                    }
                                    let nr = row + dr;
                                    let nc = col + dc;
                                    let mut nh = 0.0;
                                    if nr >= 0 && nr < logo_rows && nc >= 0 && nc < logo_cols {
                                        nh = hmap[idx(nr, nc)];
                                    }
                                    if nh < h {
                                        ex += dc as f32;
                                        ey += -(dr as f32);
                                    }
                                }
                            }
                            let el = (ex * ex + ey * ey).sqrt();
                            if el > 1e-6 {
                                ex /= el;
                                ey /= el;
                            }
                            let tn = (k as f32 / (layers - 1) as f32) * 2.0 - 1.0;
                            let side = (1.0 - tn * tn).sqrt();
                            nx.push(ex * side);
                            ny.push(ey * side);
                            nz.push(tn);
                        }
                    }
                }
                if px.len() >= MAX_POINTS {
                    break;
                }
            }
            if px.len() >= MAX_POINTS {
                break;
            }
        }
        if px.len() >= MAX_POINTS {
            break;
        }
    }

    Points {
        px,
        py,
        pz,
        nx,
        ny,
        nz,
        pcolor,
    }
}
