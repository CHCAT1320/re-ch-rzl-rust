use macroquad::prelude::*;

use crate::audio::AudioController;
use crate::chart::{Chart, LineColor, LinePoint, Note};
use crate::ease::EASE_FUNCS;
use crate::time_conv::*;

pub const CANVAS_WIDTH: f32 = 1080.0;
pub const CANVAS_HEIGHT: f32 = 1920.0;
const OFFSET_Y: f32 = 160.0 * (CANVAS_HEIGHT / 640.0);

pub struct RenderSettings {
    pub speed_value: i32,
    pub revelation_size: f64,
    pub show_debug: bool,
    pub recorder_watermark: bool,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            speed_value: 10,
            revelation_size: 1.0,
            show_debug: false,
            recorder_watermark: false,
        }
    }
}

/// 计算 speed 值（匹配网页版公式）
fn calculate_speed(speed_value: i32) -> f64 {
    (215.0 / 32.0 + speed_value as f64) * (10.0 / 129.0)
}

pub fn camera_scale(tick: f64, chart: &Chart) -> f64 {
    find_value(tick, &chart.cameraMove.scale_key_points)
}

pub fn camera_move_x(tick: f64, chart: &Chart) -> f64 {
    find_value(tick, &chart.cameraMove.x_position_key_points)
}

fn format_revelation_number(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }

    let s = format!("{:.6}", value);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn chart_to_quad_color(c: &crate::chart::Color) -> Color {
    Color::new(
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    )
}

fn interpolate_color(
    start: &crate::chart::Color,
    end: &crate::chart::Color,
    progress: f64,
) -> crate::chart::Color {
    crate::chart::Color {
        r: (start.r as f64 + (end.r as f64 - start.r as f64) * progress) as u32,
        g: (start.g as f64 + (end.g as f64 - start.g as f64) * progress) as u32,
        b: (start.b as f64 + (end.b as f64 - start.b as f64) * progress) as u32,
        a: (start.a as f64 + (end.a as f64 - start.a as f64) * progress) as u32,
    }
}

/// 标准 Alpha 混合（匹配网页版 mixColor）
fn mix_color(c1: &crate::chart::Color, c2: &crate::chart::Color) -> crate::chart::Color {
    if c2.a == 0 {
        return *c1;
    }
    if c2.a == 255 {
        return crate::chart::Color {
            a: c1.a,
            ..*c2
        };
    }
    let mix_ratio = c2.a as f64 / 255.0;
    crate::chart::Color {
        r: (c1.r as f64 * (1.0 - mix_ratio) + c2.r as f64 * mix_ratio).round() as u32,
        g: (c1.g as f64 * (1.0 - mix_ratio) + c2.g as f64 * mix_ratio).round() as u32,
        b: (c1.b as f64 * (1.0 - mix_ratio) + c2.b as f64 * mix_ratio).round() as u32,
        a: c1.a,
    }
}

fn get_current_line_color(line_color: &[LineColor], tick: f64) -> Option<crate::chart::Color> {
    if line_color.is_empty() {
        return None;
    }
    if line_color.len() == 1 {
        return Some(line_color[0].start_color);
    }

    let mut current = line_color[0].start_color;
    for i in 0..line_color.len() {
        let current_key = &line_color[i];
        let end_time = line_color
            .get(i + 1)
            .map(|next| next.time)
            .unwrap_or(current_key.time);

        if tick > end_time {
            current = current_key.end_color;
            continue;
        }

        if tick < current_key.time {
            break;
        }

        let duration = end_time - current_key.time;
        let progress = if duration > 0.0 {
            ((tick - current_key.time) / duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        current = interpolate_color(&current_key.start_color, &current_key.end_color, progress);
        break;
    }

    Some(current)
}

fn get_current_judge_ring_color(
    judge_ring_color: &[LineColor],
    tick: f64,
) -> Option<crate::chart::Color> {
    if let Some(last) = judge_ring_color.last() {
        if tick > last.time {
            return Some(last.end_color);
        }
    }

    get_current_line_color(judge_ring_color, tick)
}

fn calculate_mixed_color(
    tick: f64,
    point_color: &crate::chart::Color,
    line_color: &[LineColor],
) -> crate::chart::Color {
    match get_current_line_color(line_color, tick) {
        Some(c) => mix_color(point_color, &c),
        None => *point_color,
    }
}

fn calculate_combo(comb: u32) -> u32 {
    if comb == 0 {
        return 0;
    }
    if comb <= 5 {
        return comb;
    }
    if comb <= 8 {
        return 2 * comb - 5;
    }
    if comb <= 11 {
        return 3 * comb - 13;
    }
    4 * comb - 24
}

fn hit_effect_color(chart: &Chart, tick: f64) -> crate::chart::Color {
    let default_color = crate::chart::Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    match get_challenge_time_index(tick, chart) {
        Some(idx) => chart
            .themes
            .get(idx + 1)
            .and_then(|theme| theme.colors_list.get(2))
            .copied()
            .or_else(|| {
                chart
                    .themes
                    .get(0)
                    .and_then(|theme| theme.colors_list.get(2))
                    .copied()
            })
            .unwrap_or(default_color),
        None => chart
            .themes
            .get(0)
            .and_then(|theme| theme.colors_list.get(2))
            .copied()
            .unwrap_or(default_color),
    }
}

/// 判断线段是否与可见矩形相交。
/// 不能只判断两个端点是否在屏幕内：一条线段可能两个端点都在屏幕外，
/// 但中间穿过屏幕，这种情况仍然需要绘制。
fn segment_intersects_rect(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
) -> bool {
    // Fast accept: either endpoint inside.
    if (x0 >= left && x0 <= right && y0 >= top && y0 <= bottom)
        || (x1 >= left && x1 <= right && y1 >= top && y1 <= bottom)
    {
        return true;
    }

    // Fast reject by bounding boxes.
    if x0.max(x1) < left || x0.min(x1) > right || y0.max(y1) < top || y0.min(y1) > bottom {
        return false;
    }

    // Liang-Barsky line clipping.
    let dx = x1 - x0;
    let dy = y1 - y0;
    let mut t0 = 0.0_f64;
    let mut t1 = 1.0_f64;

    let checks = [
        (-dx, x0 - left),
        (dx, right - x0),
        (-dy, y0 - top),
        (dy, bottom - y0),
    ];

    for (p, q) in checks {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return false;
                }
                if r > t0 {
                    t0 = r;
                }
            } else {
                if r < t0 {
                    return false;
                }
                if r < t1 {
                    t1 = r;
                }
            }
        }
    }

    true
}

// ==================== Canvas ====================

pub struct CanvasRenderer {
    pub index: u32,
    pub x_position_key_points: Vec<crate::chart::KeyPoint>,
    pub speed_key_points: Vec<crate::chart::SpeedKeyPointRuntime>,
    pub x: f64,
    pub fp: f64,
}

impl CanvasRenderer {
    pub fn new(canvas_move: &crate::chart::CanvasMove) -> Self {
        Self {
            index: canvas_move.index,
            x_position_key_points: canvas_move.x_position_key_points.clone(),
            speed_key_points: canvas_move
                .speed_key_points
                .iter()
                .map(|sk| crate::chart::SpeedKeyPointRuntime {
                    time: sk.time,
                    value: sk.value,
                    fp: 0.0,
                })
                .collect(),
            x: 0.0,
            fp: 0.0,
        }
    }

    pub fn init_fp(&mut self, chart: &Chart) {
        if self.speed_key_points.is_empty() {
            return;
        }
        self.speed_key_points[0].fp = 0.0;
        for i in 1..self.speed_key_points.len() {
            let prev = &self.speed_key_points[i - 1];
            let curr = &self.speed_key_points[i];
            let time_diff = tick_to_seconds(curr.time, chart) - tick_to_seconds(prev.time, chart);
            self.speed_key_points[i].fp = prev.fp + prev.value * time_diff;
        }
    }

    pub fn speed_to_fp(&self, timer: f64, chart: &Chart) -> f64 {
        let len = self.speed_key_points.len();
        if len == 0 {
            return 0.0;
        }
        let mut target_index = len - 1;
        let mut left: isize = 0;
        let mut right: isize = (len - 1) as isize;

        while left <= right {
            let mid = ((left + right) / 2) as usize;
            let mid_time = tick_to_seconds(self.speed_key_points[mid].time, chart);
            if mid_time <= timer {
                target_index = mid;
                left = mid as isize + 1;
            } else {
                right = mid as isize - 1;
            }
        }

        let current = &self.speed_key_points[target_index];
        current.fp + (timer - tick_to_seconds(current.time, chart)) * current.value
    }

    pub fn update(&mut self, tick: f64, chart: &Chart, settings: &RenderSettings) {
        // Original web `cameraScale(tick)` returns chart camera scale * revelationSize.
        // In revelation mode this zooms the whole chart out while keeping the red
        // screen-board at the true capture area.
        let scale = camera_scale(tick, chart) * settings.revelation_size;
        let cam_x = camera_move_x(tick, chart);
        self.x =
            (find_value(tick, &self.x_position_key_points) - cam_x) * scale * CANVAS_WIDTH as f64;
        self.fp = self.speed_to_fp(tick_to_seconds(tick, chart), chart);
    }
}

// ==================== Note Runtime ====================

pub struct NoteRuntime {
    pub info: Note,
    pub line_index: usize,
    pub finded_points: (LinePoint, LinePoint),
    pub fp: f64,
    pub is_hit: bool,
    pub is_play_hit: bool,
    pub is_play_hold_end_hit: bool,
}

// ==================== Hit Effect ====================

pub struct HitEffect {
    pub x: f64,
    pub timer: f64,
    pub color: crate::chart::Color,
    pub is_bad: bool,
    pub block_count: u32,
    pub blocks_r: Vec<f64>,
    pub block_s: Vec<f64>,
    pub riztime_offsets: Vec<f64>,
    pub riztime_sizes: Vec<f64>,
}

impl HitEffect {
    fn new(
        tick_timer: f64,
        x: f64,
        color: crate::chart::Color,
        is_bad: bool,
        in_challenge_time: bool,
    ) -> Self {
        // Match the original JS behavior:
        // blockCount = Math.floor(Math.random() * 2) + 3
        // blocksR    = Math.floor(Math.random() * 361)
        // blockS     = Math.floor(Math.random() * 20) + 10
        let block_count: u32 = macroquad::rand::gen_range(3, 5);
        let mut blocks_r = Vec::new();
        let mut block_s = Vec::new();
        for _ in 0..block_count {
            blocks_r.push(macroquad::rand::gen_range(0.0, 361.0));
            block_s.push(macroquad::rand::gen_range(10.0, 30.0));
        }

        let mut riztime_offsets = Vec::new();
        let mut riztime_sizes = Vec::new();
        if in_challenge_time {
            // Original JS challenge-time extra particles:
            // for (let i = 0; i < Math.floor(Math.random() * 5) + 1; i++) {
            //     rBOffset.push(Math.random() * 440)
            //     rBS.push(Math.floor(Math.random() * 10) + 10)
            // }
            let count: i32 = macroquad::rand::gen_range(1, 6);
            for _ in 0..count {
                riztime_offsets.push(macroquad::rand::gen_range(0.0, 440.0));
                riztime_sizes.push(macroquad::rand::gen_range(10.0, 20.0));
            }
        }

        Self {
            x,
            timer: tick_timer,
            color,
            is_bad,
            block_count,
            blocks_r,
            block_s,
            riztime_offsets,
            riztime_sizes,
        }
    }
}

// ==================== Render Pipeline ====================

pub struct RenderPipeline {
    pub canvases: Vec<CanvasRenderer>,
    pub notes: Vec<NoteRuntime>,
    pub hit_effects: Vec<HitEffect>,
}

impl RenderPipeline {
    pub fn new(chart: &Chart) -> Self {
        let mut canvases = Vec::new();
        for cm in &chart.canvasMoves {
            let mut canvas = CanvasRenderer::new(cm);
            canvas.init_fp(chart);
            canvases.push(canvas);
        }

        let mut notes = Vec::new();
        for (line_index, line) in chart.lines.iter().enumerate() {
            for note in &line.notes {
                let points = &line.line_points;
                let len = points.len();
                if len == 0 {
                    continue;
                }

                let mut target_index = 0usize;
                let mut left: isize = 0;
                let mut right: isize = (len - 1) as isize;

                while left <= right {
                    let mid = ((left + right) / 2) as usize;
                    if points[mid].time <= note.time {
                        target_index = mid;
                        left = mid as isize + 1;
                    } else {
                        right = mid as isize - 1;
                    }
                }

                let p1 = points[target_index].clone();
                let p2 = if target_index + 1 < len {
                    points[target_index + 1].clone()
                } else {
                    points[target_index].clone()
                };

                let fp = canvases
                    .get(p1.canvas_index as usize)
                    .map(|canvas| canvas.speed_to_fp(tick_to_seconds(note.time, chart), chart))
                    .unwrap_or(0.0);

                notes.push(NoteRuntime {
                    info: note.clone(),
                    line_index,
                    finded_points: (p1, p2),
                    fp,
                    is_hit: false,
                    is_play_hit: false,
                    is_play_hold_end_hit: false,
                });
            }
        }

        // Sort notes by time
        notes.sort_by(|a, b| {
            a.info
                .time
                .partial_cmp(&b.info.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Self {
            canvases,
            notes,
            hit_effects: Vec::new(),
        }
    }

    // ==================== Background ====================

    pub fn draw_background(&self, chart: &Chart, tick: f64) {
        let hw = CANVAS_WIDTH / 2.0;
        let base_y = -CANVAS_HEIGHT / 2.0 - OFFSET_Y;
        let base_h = CANVAS_HEIGHT + OFFSET_Y;

        if !chart.themes.is_empty() && !chart.themes[0].colors_list.is_empty() {
            let color = &chart.themes[0].colors_list[0];
            let qc = chart_to_quad_color(color);
            draw_rectangle(-hw, base_y, CANVAS_WIDTH, base_h, qc);
        } else {
            draw_rectangle(-hw, base_y, CANVAS_WIDTH, base_h, Color::new(0.05, 0.05, 0.05, 1.0));
        }

        self.draw_riztime_background(chart, tick);
    }

    fn draw_riztime_background(&self, chart: &Chart, tick: f64) {
        let base_r = ((640.0 + 200.0) * (CANVAS_HEIGHT / 640.0)) as f64;
        let base_y = 150.0 * (CANVAS_HEIGHT / 640.0) as f64;

        // Pick the latest active challenge time independently.
        //
        // The original loop kept mutating the same radius while walking all challenge
        // times. When challenge ranges overlap (for example: challenge A is still in
        // its ending transition while challenge B has already started / is starting),
        // A's shrinking radius would affect B, so B could not be shown fully until
        // A's transition had finished. Treat those as "overlapped challenge times":
        // the later active challenge wins and its transition is computed from a clean
        // base radius.
        let active = chart
            .challengeTimes
            .iter()
            .enumerate()
            .filter(|(_, ct)| tick >= ct.start && tick <= ct.end + ct.trans_time)
            .last();

        if let Some((i, ct)) = active {
            let Some(theme) = chart.themes.get(i + 1) else {
                return;
            };
            let Some(color) = theme.colors_list.get(0) else {
                return;
            };

            let mut r = base_r;
            let mut y = base_y;

            if ct.trans_time > 0.0 {
                if tick >= ct.start && tick <= ct.start + ct.trans_time {
                    let progress = ((tick - ct.start) / ct.trans_time).clamp(0.0, 1.0);
                    r = base_r * EASE_FUNCS[2](progress);
                } else if tick >= ct.end && tick <= ct.end + ct.trans_time {
                    let progress = ((tick - ct.end) / ct.trans_time).clamp(0.0, 1.0);
                    r = base_r * (1.0 - EASE_FUNCS[3](progress));
                    y = -CANVAS_HEIGHT as f64 / 2.0 - OFFSET_Y as f64;
                }
            }

            draw_circle(0.0, y as f32, r.max(0.0) as f32, chart_to_quad_color(color));
        }
    }

    fn draw_cover(&self, chart: &Chart) {
        if !chart.themes.is_empty() && !chart.themes[0].colors_list.is_empty() {
            let color = &chart.themes[0].colors_list[0];
            let base_r = color.r as f32 / 255.0;
            let base_g = color.g as f32 / 255.0;
            let base_b = color.b as f32 / 255.0;

            let hw = CANVAS_WIDTH / 2.0;
            let bar_h = 30.0 * (CANVAS_HEIGHT / 640.0);
            let times = 30.0 * (CANVAS_HEIGHT / 640.0);

            for step in 0..5 {
                let i = step as f32 * 0.2;
                let alpha = (step as f32) / 5.0;
                let c = Color::new(base_r, base_g, base_b, alpha);

                let top_y = -CANVAS_HEIGHT / 2.0 - (170.0 * (CANVAS_HEIGHT / 640.0) + times * i);
                draw_rectangle(-hw, top_y, CANVAS_WIDTH, bar_h, c);

                let bot_y = -CANVAS_HEIGHT / 2.0
                    - (230.0 * (CANVAS_HEIGHT / 640.0) - times * i)
                    + CANVAS_HEIGHT
                    - 70.0 * (CANVAS_HEIGHT / 640.0);
                draw_rectangle(-hw, bot_y, CANVAS_WIDTH, bar_h, c);
            }
        }
    }

    // ==================== Lines ====================

    pub fn draw_lines(
        &mut self,
        chart: &Chart,
        tick: f64,
        settings: &RenderSettings,
        font: Option<&Font>,
    ) {
        let speed = calculate_speed(settings.speed_value);
        let scale = camera_scale(tick, chart) * settings.revelation_size;
        let screen_w = CANVAS_WIDTH as f64;
        let screen_h = CANVAS_HEIGHT as f64;
        // Match the actual camera world rect:
        // main.rs uses Rect(left=-540, top=-960-OFFSET_Y, h=1920), so only this range is visible.
        // Add a small margin for thick lines / judge rings, but do not draw far off-screen geometry.
        let cull_margin = 80.0_f64;
        let visible_left = -(screen_w / 2.0) - cull_margin;
        let visible_right = screen_w / 2.0 + cull_margin;
        let visible_top = -(screen_h / 2.0) - OFFSET_Y as f64 - cull_margin;
        let visible_bottom = visible_top + screen_h + cull_margin * 2.0;
        let line_width = (7.0 * scale).max(3.0) as f32;
        // 判定圈略大于音符头，保持可读性但不压过画面。
        let judge_ring_radius = (38.0 * scale).max(18.0) as f32;
        let judge_ring_width = (7.0 * scale).max(3.0) as f32;

        let mut line_order: Vec<usize> = (0..chart.lines.len()).collect();
        line_order.sort_by(|&a, &b| {
            let at = chart.lines[a]
                .line_points
                .first()
                .map(|p| p.time)
                .unwrap_or(f64::INFINITY);
            let bt = chart.lines[b]
                .line_points
                .first()
                .map(|p| p.time)
                .unwrap_or(f64::INFINITY);
            at.partial_cmp(&bt).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Judge rings must be rendered above all line bodies, otherwise later lines can cover them.
        let mut judge_rings: Vec<(f32, Color)> = Vec::new();

        // Revelation mode overlays every line control point as a black dot on top of the line graph.
        let mut revelation_points: Vec<(f32, f32)> = Vec::new();

        for line_index in line_order {
            let line = &chart.lines[line_index];
            let points = &line.line_points;
            if points.len() < 2 {
                continue;
            }

            let line_current_color = get_current_line_color(&line.line_color, tick);

            if settings.revelation_size != 1.0 {
                for point in points {
                    let ci = point.canvas_index as usize;
                    if ci >= self.canvases.len() {
                        continue;
                    }
                    let canvas = &self.canvases[ci];
                    let fp = point
                        .fp
                        .unwrap_or_else(|| canvas.speed_to_fp(tick_to_seconds(point.time, chart), chart));
                    let x = point.x_position * scale * screen_w + canvas.x;
                    let y = -(fp - canvas.fp) * screen_h * speed * scale;

                    if x < visible_left || x > visible_right || y < visible_top || y > visible_bottom {
                        continue;
                    }

                    revelation_points.push((x as f32, y as f32));
                }
            }

            for i in 0..(points.len() - 1) {
                let point = &points[i];
                let ci = point.canvas_index as usize;
                if ci >= self.canvases.len() {
                    continue;
                }
                let canvas = &self.canvases[ci];

                let fp = point
                    .fp
                    .unwrap_or_else(|| canvas.speed_to_fp(tick_to_seconds(point.time, chart), chart));

                let x = point.x_position * scale * screen_w + canvas.x;
                let y = -(fp - canvas.fp) * screen_h * speed * scale;

                let next_point = points.get(i + 1);
                if let Some(np) = next_point {
                    let nci = np.canvas_index as usize;
                    if nci >= self.canvases.len() {
                        continue;
                    }
                    let next_canvas = &self.canvases[nci];
                    let next_fp = np
                        .fp
                        .unwrap_or_else(|| next_canvas.speed_to_fp(tick_to_seconds(np.time, chart), chart));
                    let x1 = np.x_position * scale * screen_w + next_canvas.x;
                    let y1 = -(next_fp - next_canvas.fp) * screen_h * speed * scale;

                    if !segment_intersects_rect(
                        x,
                        y,
                        x1,
                        y1,
                        visible_left,
                        visible_top,
                        visible_right,
                        visible_bottom,
                    ) {
                        continue;
                    }

                    let c0 = match line_current_color {
                        Some(line_color) => mix_color(&point.color, &line_color),
                        None => point.color,
                    };
                    let c1 = match line_current_color {
                        Some(line_color) => mix_color(&np.color, &line_color),
                        None => np.color,
                    };

                    // If two adjacent points resolve to the same screen position, draw a point instead
                    // of a zero-length line segment. This keeps stationary line points visible.
                    if (x - x1).abs() < 0.001 && (y - y1).abs() < 0.001 {
                        draw_circle(
                            x as f32,
                            y as f32,
                            (line_width * 0.65).max(2.0),
                            chart_to_quad_color(&c0),
                        );
                        continue;
                    }

                    let ease_fn = if point.ease_type < EASE_FUNCS.len() as u32 {
                        EASE_FUNCS[point.ease_type as usize]
                    } else {
                        EASE_FUNCS[0]
                    };

                    // Lower subdivision count for realtime playback. The original 16-step path
                    // is expensive because each chart segment becomes 16 draw calls; 8 keeps
                    // easing/color gradients visible while roughly halving line draw calls.
                    const LINE_STEPS: usize = 8;
                    let mut prev_x = x as f32;
                    let mut prev_y = y as f32;

                    let r0 = c0.r as f32 / 255.0;
                    let g0 = c0.g as f32 / 255.0;
                    let b0 = c0.b as f32 / 255.0;
                    let a0 = c0.a as f32 / 255.0;
                    let dr = (c1.r as f32 - c0.r as f32) / 255.0 / LINE_STEPS as f32;
                    let dg = (c1.g as f32 - c0.g as f32) / 255.0 / LINE_STEPS as f32;
                    let db = (c1.b as f32 - c0.b as f32) / 255.0 / LINE_STEPS as f32;
                    let da = (c1.a as f32 - c0.a as f32) / 255.0 / LINE_STEPS as f32;

                    for s in 1..=LINE_STEPS {
                        let t = s as f64 / LINE_STEPS as f64;
                        let pos_ease = ease_fn(t);
                        let cx = (x + pos_ease * (x1 - x)) as f32;
                        let cy = (y + t * (y1 - y)) as f32;
                        let sf = (s - 1) as f32;
                        let color = Color::new(
                            r0 + dr * sf,
                            g0 + dg * sf,
                            b0 + db * sf,
                            a0 + da * sf,
                        );

                        draw_line(prev_x, prev_y, cx, cy, line_width, color);

                        prev_x = cx;
                        prev_y = cy;
                    }

                    // Draw judge circle
                    if tick >= point.time && tick < np.time {
                        let time_diff = np.time - point.time;
                        if time_diff > 0.0 {
                            let progress = ((tick - point.time) / time_diff).max(0.0).min(1.0);
                            let ease_val = ease_fn(progress);
                            let cx = (x + ease_val * (x1 - x)) as f32;

                            if !line.judge_ring_color.is_empty() {
                                if let Some(jc) =
                                    get_current_judge_ring_color(&line.judge_ring_color, tick)
                                {
                                    // 判定圈颜色独立使用 judgeRingColor 当前关键帧颜色。
                                    // 不再与 lineColor 混合；若当前 tick 小于第一个颜色事件 time，
                                    // get_current_line_color 会返回第一个事件的 startColor。
                                    // 若当前 tick 大于最后一个颜色事件 time，则固定为最后事件的 endColor。
                                    let ring_color = chart_to_quad_color(&jc);
                                    judge_rings.push((cx, ring_color));
                                }
                            } else {
                                judge_rings.push((cx, WHITE));
                            }
                        }
                    }
                } else {
                    if x < visible_left || x > visible_right || y < visible_top || y > visible_bottom {
                        continue;
                    }

                    let c0 = calculate_mixed_color(tick, &point.color, &line.line_color);
                    let qc = chart_to_quad_color(&c0);
                    draw_circle(x as f32, y as f32, (2.0 * scale) as f32, qc);
                }
            }
        }

        for (cx, color) in judge_rings {
            draw_circle_lines(cx, 0.0, judge_ring_radius, judge_ring_width, color);
        }

        if settings.revelation_size != 1.0 {
            let point_radius = (5.0 * scale * (screen_w / 360.0)).max(4.0) as f32;
            for (x, y) in revelation_points {
                draw_circle(x, y, point_radius, BLACK);
            }

            // Match the original web revelation overlay: draw each canvas index at
            // the current moving canvas X position, so the number follows canvas movement.
            let font_size = (35.0 * scale * (screen_w / 360.0)).max(18.0) as f32;
            let y = (200.0 * scale * (screen_h / 540.0)) as f32;
            for canvas in &self.canvases {
                let text = canvas.index.to_string();
                let width = measure_text(&text, font, font_size as u16, 1.0).width;
                let x = canvas.x as f32 - width / 2.0;
                let outline = 1.5_f32;
                let draw_canvas_index = |dx: f32, dy: f32, color: Color| {
                    if let Some(font) = font {
                        draw_text_ex(
                            &text,
                            x + dx,
                            y + dy,
                            TextParams {
                                font: Some(font),
                                font_size: font_size as u16,
                                font_scale: 1.0,
                                color,
                                ..Default::default()
                            },
                        );
                    } else {
                        draw_text(&text, x + dx, y + dy, font_size, color);
                    }
                };
                draw_canvas_index(-outline, 0.0, WHITE);
                draw_canvas_index(outline, 0.0, WHITE);
                draw_canvas_index(0.0, -outline, WHITE);
                draw_canvas_index(0.0, outline, WHITE);
                draw_canvas_index(0.0, 0.0, BLACK);
            }
        }
    }

    // ==================== Notes ====================

    pub fn draw_notes(
        &mut self,
        chart: &Chart,
        tick: f64,
        settings: &RenderSettings,
        audio: &mut AudioController,
        font: Option<&Font>,
    ) {
        let speed = calculate_speed(settings.speed_value);
        let scale = camera_scale(tick, chart) * settings.revelation_size;
        let screen_w = CANVAS_WIDTH as f64;
        let screen_h = CANVAS_HEIGHT as f64;

        let mut hit_count = 0u32;

        for note in &mut self.notes {
            // Clone info to avoid borrow conflicts with self.canvases
            let info = note.info.clone();
            let (ref p1, ref p2) = note.finded_points;
            let ci = p1.canvas_index as usize;
            let ci2 = p2.canvas_index as usize;
            if ci >= self.canvases.len() || ci2 >= self.canvases.len() {
                continue;
            }
            let canvas = &self.canvases[ci];
            let canvas2 = &self.canvases[ci2];

            let ease_fn = if p1.ease_type < EASE_FUNCS.len() as u32 {
                EASE_FUNCS[p1.ease_type as usize]
            } else {
                EASE_FUNCS[0]
            };

            let point_x = p1.x_position * scale * screen_w + canvas.x;
            let next_x = p2.x_position * scale * screen_w + canvas2.x;
            let time_diff = p2.time - p1.time;

            let ease_val = if time_diff > 0.0 {
                ease_fn((info.time - p1.time) / time_diff)
            } else {
                0.0
            };

            let mut x = point_x + ease_val * (next_x - point_x);
            if info.time == p1.time {
                x = point_x;
            }
            if info.time == p2.time {
                x = next_x;
            }

            if tick > info.time {
                if let Some((point1, point2)) = Self::find_line_points_at_tick(chart, note.line_index, tick) {
                    let canvas1_idx = point1.canvas_index as usize;
                    let canvas2_idx = point2.canvas_index as usize;
                    if canvas1_idx < self.canvases.len() && canvas2_idx < self.canvases.len() {
                        let canvas1 = &self.canvases[canvas1_idx];
                        let canvas2 = &self.canvases[canvas2_idx];
                        let dynamic_time_diff = point2.time - point1.time;
                        let dynamic_ease_fn = if point1.ease_type < EASE_FUNCS.len() as u32 {
                            EASE_FUNCS[point1.ease_type as usize]
                        } else {
                            EASE_FUNCS[0]
                        };
                        let dynamic_ease = if dynamic_time_diff > 0.0 {
                            dynamic_ease_fn((tick - point1.time) / dynamic_time_diff)
                        } else {
                            0.0
                        };
                        let x1 = point1.x_position * scale * screen_w + canvas1.x;
                        let x2 = point2.x_position * scale * screen_w + canvas2.x;
                        x = x1 + dynamic_ease * (x2 - x1);
                    }
                }
            }

            // Hit sound + visual hit effect.
            if !note.is_hit && !note.is_play_hit && tick >= info.time {
                note.is_play_hit = true;
                audio.play_hit_sound(info.note_type);

                let challenge_idx = get_challenge_time_index(tick, chart);
                let default_color = crate::chart::Color {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                };
                // Match the original JS hit color:
                // challengeTimeIndex === -1 ? themes[0].colorsList[2] : themes[challengeTimeIndex].colorsList[2]
                // Rust get_challenge_time_index returns 0-based challenge index, while JS returns theme index i + 1.
                let effect_color = match challenge_idx {
                    Some(idx) => {
                        let theme_idx = idx + 1;
                        chart
                            .themes
                            .get(theme_idx)
                            .and_then(|theme| theme.colors_list.get(2))
                            .copied()
                            .or_else(|| {
                                chart
                                    .themes
                                    .get(0)
                                    .and_then(|theme| theme.colors_list.get(2))
                                    .copied()
                            })
                            .unwrap_or(default_color)
                    }
                    None => chart
                        .themes
                        .get(0)
                        .and_then(|theme| theme.colors_list.get(2))
                        .copied()
                        .unwrap_or(default_color),
                };

                let timer = tick_to_seconds(tick, chart);
                self.hit_effects.push(HitEffect::new(
                    timer,
                    x,
                    effect_color,
                    false,
                    get_challenge_time_index(tick, chart).is_some(),
                ));
            }

            if info.note_type == 2 && !info.other_informations.is_empty() {
                let end_time = info.other_informations[0];
                if tick < end_time {
                    note.is_play_hold_end_hit = false;
                } else if !note.is_play_hold_end_hit {
                    note.is_play_hold_end_hit = true;
                    let effect_color = hit_effect_color(chart, tick);
                    let timer = tick_to_seconds(tick, chart);
                    self.hit_effects.push(HitEffect::new(
                        timer,
                        x,
                        effect_color,
                        false,
                        get_challenge_time_index(tick, chart).is_some(),
                    ));
                }
            }

            if tick < info.time {
                note.is_hit = false;
                note.is_play_hit = false;
                note.is_play_hold_end_hit = false;
            } else {
                note.is_hit = true;
            }
            if note.is_hit {
                hit_count += 1;
                if info.note_type == 2
                    && !info.other_informations.is_empty()
                    && tick >= info.other_informations[0]
                {
                    hit_count += 1;
                }
            }

            // Skip drawing notes that are already fully resolved, but only after
            // hit/hold-end sound effects and combo state have been updated. This
            // prevents off-screen/expired optimization from swallowing hit events
            // when a frame jumps past the note or hold end time.
            if note.is_hit && tick > info.time && info.note_type != 2 {
                continue;
            }
            if note.is_hit
                && info.note_type == 2
                && !info.other_informations.is_empty()
                && tick >= info.other_informations[0] + 0.5
            {
                continue;
            }

            // Calculate Y position
            let mut y = -(note.fp - canvas.fp) * screen_h * speed * scale;

            // Match the original JS note culling: only skip notes far above the canvas.
            // Do not bottom-cull here because the camera transform already clips drawing,
            // and overly strict culling can hide notes before they enter the screen.
            if y < -screen_h {
                continue;
            }

            if info.note_type == 2
                && tick >= info.time
                && !info.other_informations.is_empty()
                && tick <= info.other_informations[0] + 0.5
            {
                y = 0.0;
            }

            // Cull notes outside the actual render camera world rect before doing
            // theme/color/body draw work. This preserves hit/combo state above,
            // but skips GPU draw calls for notes that cannot appear on screen.
            let cull_margin = (90.0 * scale.max(1.0) * (screen_w / 360.0)).max(90.0);
            let visible_left = -(screen_w / 2.0) - cull_margin;
            let visible_right = screen_w / 2.0 + cull_margin;
            let visible_top = -(screen_h / 2.0) - OFFSET_Y as f64 - cull_margin;
            let visible_bottom = visible_top + screen_h + cull_margin * 2.0;

            if info.note_type == 2 {
                let hold_visible = Self::calculate_hold_end(
                    &self.canvases,
                    tick,
                    chart,
                    scale,
                    &info,
                    settings,
                )
                .map(|(end_y, _)| {
                    x >= visible_left
                        && x <= visible_right
                        && y.max(end_y) >= visible_top
                        && y.min(end_y) <= visible_bottom
                })
                .unwrap_or_else(|| {
                    x >= visible_left && x <= visible_right && y >= visible_top && y <= visible_bottom
                });

                if !hold_visible {
                    continue;
                }
            } else if x < visible_left || x > visible_right || y < visible_top || y > visible_bottom {
                continue;
            }

            // Get color from theme
            let default_color = crate::chart::Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            };
            let challenge_idx = get_challenge_time_index(tick, chart);
            let color = match challenge_idx {
                Some(idx) => {
                    if idx + 1 < chart.themes.len() && chart.themes[idx + 1].colors_list.len() > 1 {
                        chart.themes[idx + 1].colors_list[1]
                    } else {
                        chart.themes[0].colors_list.get(0).copied().unwrap_or(default_color)
                    }
                }
                None => {
                    if !chart.themes.is_empty() && chart.themes[0].colors_list.len() > 1 {
                        chart.themes[0].colors_list[1]
                    } else {
                        chart.themes[0].colors_list.get(0).copied().unwrap_or(default_color)
                    }
                }
            };

            // Calculate note size
            let hold_scale = Self::get_hold_head_scale(tick, &info);
            let wh = if info.note_type == 1 {
                // Drag 比 Tap 小一点。
                18.0 * scale * hold_scale * (screen_w / 360.0)
            } else if info.note_type == 2 {
                23.0 * scale * hold_scale * (screen_w / 360.0)
            } else {
                25.0 * scale * hold_scale * (screen_w / 360.0)
            };

            let note_color = if info.note_type == 1 || info.note_type == 2 {
                WHITE
            } else {
                chart_to_quad_color(&color)
            };

            // Draw Hold body before the head so it never covers the visible note head.
            if info.note_type == 2 {
                Self::draw_hold_body_from_canvases(
                    &self.canvases,
                    tick,
                    chart,
                    x,
                    y,
                    scale,
                    &color,
                    &info,
                    settings,
                );
            }

            // Draw note head on top of hold body.
            // Keep the total visual note size unchanged: draw the black border
            // inside the original radius, then draw the colored/white fill smaller.
            let note_radius = (wh / 2.0) as f32;
            let border_base = if info.note_type == 1 { 3.0 } else { 5.0 };
            let border_width = (border_base * scale * (screen_w / 360.0)) as f32;
            let inner_radius = (note_radius - border_width).max(0.0);
            draw_circle(x as f32, y as f32, note_radius, BLACK);
            draw_circle(x as f32, y as f32, inner_radius, note_color);
        }

        // ==================== Combo ====================
        let combo = calculate_combo(hit_count);
        if combo > 0 {
            let font_size = 36.0 * (CANVAS_WIDTH / 360.0);
            let catplay_size = 22.0 * (CANVAS_WIDTH / 360.0);
            let y_pos = -CANVAS_HEIGHT / 2.0 - 130.0 * (CANVAS_HEIGHT / 640.0);
            let margin = 26.0 * (CANVAS_WIDTH / 360.0);

            let combo_str = combo.to_string();
            let combo_width = measure_text(&combo_str, font, font_size as u16, 1.0).width;
            let catplay_width = measure_text("CATPLAY", font, catplay_size as u16, 1.0).width;
            let x_pos = CANVAS_WIDTH / 2.0 - combo_width - margin;
            let catplay_x = (x_pos - catplay_width - 8.0).max(-CANVAS_WIDTH / 2.0 + margin);

            let outline = 2.0_f32;
            let draw_ui_text = |text: &str, x: f32, y: f32, size: f32, color: Color| {
                let params = |c: Color| TextParams {
                    font,
                    font_size: size as u16,
                    font_scale: 1.0,
                    color: c,
                    ..Default::default()
                };
                if font.is_some() {
                    draw_text_ex(text, x - outline, y, params(WHITE));
                    draw_text_ex(text, x + outline, y, params(WHITE));
                    draw_text_ex(text, x, y - outline, params(WHITE));
                    draw_text_ex(text, x, y + outline, params(WHITE));
                    draw_text_ex(text, x, y, params(color));
                } else {
                    draw_text(text, x - outline, y, size, WHITE);
                    draw_text(text, x + outline, y, size, WHITE);
                    draw_text(text, x, y - outline, size, WHITE);
                    draw_text(text, x, y + outline, size, WHITE);
                    draw_text(text, x, y, size, color);
                }
            };

            draw_ui_text(&combo_str, x_pos, y_pos, font_size, BLACK);
            draw_ui_text("CATPLAY", catplay_x, y_pos, catplay_size, BLACK);
        }

        // ==================== Watermark ====================
        let screen_h = CANVAS_HEIGHT;
        let y = -screen_h / 2.0 + 150.0 * (screen_h / 640.0);
        let app_name = if settings.recorder_watermark {
            "RECORDER"
        } else {
            "PLAYER"
        };
        let watermark = if settings.revelation_size == 1.0 {
            format!("RE:CH-RZL-RUST {} v0.1.0 by CHCAT1320", app_name)
        } else {
            format!(
                "CHART REVELATION : RE:CH-RZL-RUST {} VERSION 0.1.0 ALL CODE BY CHCAT1320",
                app_name
            )
        };
        let base_font_size = if settings.revelation_size == 1.0 {
            18.0
        } else {
            12.0
        } * (CANVAS_WIDTH / 360.0);

        // Keep the centered watermark inside the visible canvas width.
        // It should be close to full width in revelation mode, but still leave side margins.
        let max_wm_width = CANVAS_WIDTH * 0.92;
        let measured = measure_text(&watermark, font, base_font_size as u16, 1.0).width;
        let fit_scale = if measured > max_wm_width && measured > 0.0 {
            max_wm_width / measured
        } else {
            1.0
        };
        let font_size = base_font_size * fit_scale;
        let wm_width = measure_text(&watermark, font, font_size as u16, 1.0).width;
        let wm_x = -wm_width / 2.0;

        if let Some(font) = font {
            draw_text_ex(
                &watermark,
                wm_x,
                y,
                TextParams {
                    font: Some(font),
                    font_size: font_size as u16,
                    font_scale: 1.0,
                    color: WHITE,
                    ..Default::default()
                },
            );
        } else {
            draw_text(&watermark, wm_x, y, font_size, WHITE);
        }

        // ==================== Screen board (debug border) ====================
        if settings.revelation_size != 1.0 {
            let rev = settings.revelation_size as f32;
            let hw = CANVAS_WIDTH / 2.0 * rev;
            let hh = CANVAS_HEIGHT / 2.0 * rev;
            let off = OFFSET_Y * rev;
            draw_rectangle_lines(
                -hw,
                -hh - off,
                CANVAS_WIDTH * rev,
                CANVAS_HEIGHT * rev,
                2.0,
                RED,
            );
        }

        // ==================== Hit Effects ====================
        let timer = tick_to_seconds(tick, chart);
        self.hit_effects.retain(|effect| {
            let dt = timer - effect.timer;
            if dt > 0.7 || dt < 0.0 {
                return false;
            }

            let t = dt / 0.7;
            let ease_val = EASE_FUNCS[11](t); // easeOutQuint

            let size =
                (30.0 + 70.0 * ease_val * (screen_w / 360.0)) * scale / 2.0;
            let line_w = (30.0 - 30.0 * ease_val)
                .max(0.0)
                * scale
                * (screen_w / 360.0);
            let alpha = (1.0 - ease_val).max(0.0);

            let effect_color = if effect.is_bad {
                Color::new(0.0, 0.0, 0.0, alpha as f32)
            } else {
                Color::new(
                    effect.color.r as f32 / 255.0,
                    effect.color.g as f32 / 255.0,
                    effect.color.b as f32 / 255.0,
                    alpha as f32,
                )
            };

            draw_circle_lines(
                effect.x as f32,
                0.0,
                size as f32,
                line_w as f32,
                effect_color,
            );

            for i in 0..effect.block_count as usize {
                let angle = effect.blocks_r[i] * std::f64::consts::PI / 180.0;
                let wh = effect.block_s[i] * scale * (screen_w / 360.0);
                let block_offset = ease_val * 100.0 * scale * (screen_w / 360.0);
                let bx = effect.x + block_offset * angle.cos();
                let by = block_offset * angle.sin();
                let block_decay = EASE_FUNCS[10](t);
                let ba = (1.0 - block_decay).max(0.0);

                let block_color = if effect.is_bad {
                    Color::new(0.0, 0.0, 0.0, ba as f32)
                } else {
                    Color::new(
                        effect.color.r as f32 / 255.0,
                        effect.color.g as f32 / 255.0,
                        effect.color.b as f32 / 255.0,
                        ba as f32,
                    )
                };

                draw_circle(
                    bx as f32,
                    by as f32,
                    (wh * (1.0 - block_decay) / 2.0) as f32,
                    block_color,
                );
            }

            // Challenge time extra particles from the original web version:
            // drawRiztimeBolock() emits 1..5 small circles that fly vertically upward
            // from the hit position while shrinking/fading.
            for i in 0..effect.riztime_offsets.len() {
                let wh = effect.riztime_sizes[i] * scale * (screen_w / 360.0);
                let offset = wh / 2.0;
                let block_offset = ease_val * effect.riztime_offsets[i] * scale * (screen_w / 360.0);
                let by = -(block_offset - offset);
                let block_decay = EASE_FUNCS[10](t);
                let ba = (1.0 - block_decay).max(0.0);

                let block_color = if effect.is_bad {
                    Color::new(0.0, 0.0, 0.0, ba as f32)
                } else {
                    Color::new(
                        effect.color.r as f32 / 255.0,
                        effect.color.g as f32 / 255.0,
                        effect.color.b as f32 / 255.0,
                        ba as f32,
                    )
                };

                draw_circle(
                    effect.x as f32,
                    by as f32,
                    (wh * (1.0 - block_decay) / 2.0) as f32,
                    block_color,
                );
            }

            true
        });
    }

    fn draw_hold_body_from_canvases(
        canvases: &[CanvasRenderer],
        tick: f64,
        chart: &Chart,
        x: f64,
        y: f64,
        scale: f64,
        color: &crate::chart::Color,
        note: &Note,
        settings: &RenderSettings,
    ) {
        if note.other_informations.is_empty() {
            return;
        }
        let end_time = note.other_informations[0];
        if tick > end_time {
            return;
        }

        let screen_w = CANVAS_WIDTH as f64;

        // Get end canvas index from other_informations
        let end_canvas_idx = if note.other_informations.len() > 1 {
            note.other_informations[1] as usize
        } else {
            0
        };

        if end_canvas_idx >= canvases.len() {
            return;
        }

        let end_fp = canvases[end_canvas_idx].speed_to_fp(tick_to_seconds(end_time, chart), chart);

        let canvas = &canvases[end_canvas_idx];
        let speed = calculate_speed(settings.speed_value);
        let end_y = (canvas.fp - end_fp) * CANVAS_HEIGHT as f64 * speed * scale;

        let h = end_y - y;
        if h.abs() < 0.1 {
            return;
        }
        let w = 14.0 * scale * (screen_w / 360.0);

        let qc = chart_to_quad_color(color);

        // Fill: keep the first 2/3 of the hold body opaque, then fade the final
        // 1/3 from opaque to fully transparent toward the tail.
        let fade_start = y + h * 0.6;
        let opaque_y = y.min(fade_start);
        let opaque_h = (fade_start - y).abs();
        if opaque_h > 0.1 {
            draw_rectangle(
                (x - w / 2.0) as f32,
                opaque_y as f32,
                w as f32,
                opaque_h as f32,
                qc,
            );
        }

        const HOLD_FADE_STEPS: usize = 24;
        for i in 0..HOLD_FADE_STEPS {
            let t0 = i as f64 / HOLD_FADE_STEPS as f64;
            let t1 = (i + 1) as f64 / HOLD_FADE_STEPS as f64;
            let seg_y0 = fade_start + (end_y - fade_start) * t0;
            let seg_y1 = fade_start + (end_y - fade_start) * t1;
            let seg_y = seg_y0.min(seg_y1);
            let seg_h = (seg_y1 - seg_y0).abs();

            if seg_h <= 0.1 {
                continue;
            }

            let alpha = (1.0 - (t0 + t1) * 0.5).clamp(0.0, 1.0) as f32;
            draw_rectangle(
                (x - w / 2.0) as f32,
                seg_y as f32,
                w as f32,
                seg_h as f32,
                Color::new(qc.r, qc.g, qc.b, qc.a * alpha),
            );
        }

        // Borders
        let border_w = (3.0 * scale * (screen_w / 360.0)) as f32;
        draw_line(
            (x - w / 2.0) as f32,
            y as f32,
            (x - w / 2.0) as f32,
            end_y as f32,
            border_w,
            BLACK,
        );
        draw_line(
            (x + w / 2.0) as f32,
            y as f32,
            (x + w / 2.0) as f32,
            end_y as f32,
            border_w,
            BLACK,
        );
    }

    fn find_line_points_at_tick(chart: &Chart, line_index: usize, tick: f64) -> Option<(LinePoint, LinePoint)> {
        let line = chart.lines.get(line_index)?;
        let points = &line.line_points;
        if points.is_empty() {
            return None;
        }

        let mut target_index = points.len() - 1;
        let mut left: isize = 0;
        let mut right: isize = (points.len() - 1) as isize;

        while left <= right {
            let mid = ((left + right) / 2) as usize;
            if points[mid].time <= tick {
                target_index = mid;
                left = mid as isize + 1;
            } else {
                right = mid as isize - 1;
            }
        }

        let p1 = points[target_index].clone();
        let p2 = points.get(target_index + 1).cloned().unwrap_or_else(|| p1.clone());
        Some((p1, p2))
    }

    /// Calculate hold body end position given canvas data (avoids borrow issues)
    fn calculate_hold_end(
        canvases: &[CanvasRenderer],
        tick: f64,
        chart: &Chart,
        scale: f64,
        note: &Note,
        settings: &RenderSettings,
    ) -> Option<(f64, f64)> {
        if note.other_informations.is_empty() {
            return None;
        }
        let end_time = note.other_informations[0];
        if tick > end_time {
            return None;
        }

        let end_canvas_idx = if note.other_informations.len() > 1 {
            note.other_informations[1] as usize
        } else {
            0
        };

        if end_canvas_idx >= canvases.len() {
            return None;
        }

        let end_fp = canvases[end_canvas_idx]
            .speed_to_fp(tick_to_seconds(end_time, chart), chart);

        let canvas = &canvases[end_canvas_idx];
        let speed = calculate_speed(settings.speed_value);
        let end_y =
            (canvas.fp - end_fp) * CANVAS_HEIGHT as f64 * speed * scale;

        Some((end_y, end_fp))
    }

    fn get_hold_head_scale(tick: f64, note: &Note) -> f64 {
        if note.note_type != 2 {
            return 1.0;
        }
        if note.other_informations.is_empty() {
            return 1.0;
        }
        let end_time = note.other_informations[0];
        if tick < end_time {
            return 1.0;
        }
        if tick >= end_time && tick <= end_time + 0.5 {
            (1.0 - EASE_FUNCS[1]((tick - end_time) / 0.5)).max(0.0)
        } else {
            0.0
        }
    }

    // ==================== Revelation Info (Debug) ====================

    pub fn draw_revelation_info(
        &self,
        chart: &Chart,
        tick: f64,
        settings: &RenderSettings,
        font: Option<&Font>,
    ) {
        if !settings.show_debug && settings.revelation_size == 1.0 {
            return;
        }

        let screen_w = CANVAS_WIDTH;
        let screen_h = CANVAS_HEIGHT;
        let font_size = 12.0 * (screen_w / 360.0);
        // Revelation/debug chart info uses the original left-side position, but
        // shifted down a bit to avoid crowding the very top of the screen.
        let x = -170.0 * (screen_w / 360.0);
        let mut y = -screen_h / 2.0 - 130.0 * (screen_h / 640.0);
        let h = font_size + 6.0;

        let mut move_count = 0u32;
        let mut speed_count = 0u32;
        for c in &self.canvases {
            move_count += c.x_position_key_points.len() as u32;
            speed_count += c.speed_key_points.len() as u32;
        }

        let mut point_count = 0u32;
        for line in &chart.lines {
            point_count += line.line_points.len() as u32;
        }

        let info_lines: Vec<String> = vec![
            format!("Canvas count: {}", self.canvases.len()),
            format!("Canvas move events: {}", move_count),
            format!("Canvas speed events: {}", speed_count),
            format!("Lines: {}", chart.lines.len()),
            format!("Points: {}", point_count),
            format!("Notes: {}", self.notes.len()),
            format!(
                "Camera scale: {}",
                format_revelation_number(camera_scale(tick, chart))
            ),
            format!(
                "Revelation scale: {}",
                format_revelation_number(settings.revelation_size)
            ),
            format!(
                "Cam scale events: {}",
                chart.cameraMove.scale_key_points.len()
            ),
            format!(
                "Cam move events: {}",
                chart.cameraMove.x_position_key_points.len()
            ),
            format!(
                "Camera X: {}",
                format_revelation_number(camera_move_x(tick, chart))
            ),
            format!("Challenge time count: {}", chart.challengeTimes.len()),
            format!("Speed: {}", settings.speed_value),
        ];

        for info in &info_lines {
            // Manual outline since draw_text_outline unavailable in macroquad 0.4.
            // Use the same loaded font as the UI to avoid mixed default/rizline fonts.
            let o = 0.5_f32;
            let draw = |dx: f32, dy: f32, color: Color| {
                if let Some(font) = font {
                    draw_text_ex(
                        info,
                        x + dx,
                        y + dy,
                        TextParams {
                            font: Some(font),
                            font_size: font_size as u16,
                            font_scale: 1.0,
                            color,
                            ..Default::default()
                        },
                    );
                } else {
                    draw_text(info, x + dx, y + dy, font_size, color);
                }
            };
            draw(-o, 0.0, BLACK);
            draw(o, 0.0, BLACK);
            draw(0.0, -o, BLACK);
            draw(0.0, o, BLACK);
            draw(0.0, 0.0, WHITE);
            y += h;
        }
    }
}
