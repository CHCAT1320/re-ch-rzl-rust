#![allow(dead_code, non_snake_case)]

mod chart;
mod ease;
mod time_conv;
mod render;
mod audio;

use macroquad::prelude::*;
use chart::Chart;
use render::{RenderPipeline, RenderSettings, CANVAS_WIDTH, CANVAS_HEIGHT};
use audio::AudioController;
use time_conv::{seconds_to_tick, tick_to_seconds};
use std::path::{Path, PathBuf};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const WINDOW_W: u32 = 450;
// Keep enough room for title bar + common Windows taskbar while staying close
// to centered on 1080p displays.
const WINDOW_H: u32 = 780;
const RENDER_FPS: u32 = 60;

#[derive(Debug, Clone)]
struct RenderExportConfig {
    chart_path: String,
    bgm_path: String,
    output_path: String,
    fps: u32,
    revelation_size: f64,
    hw_encoder: Option<String>,
}

fn parse_render_export_config() -> Option<RenderExportConfig> {
    let args: Vec<String> = std::env::args().collect();
    let render_pos = args.iter().position(|arg| arg == "--render" || arg == "render")?;

    let mut fps = RENDER_FPS;
    let mut revelation_size = 1.0;
    let mut hw_encoder: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();

    let mut i = render_pos + 1;
    while i < args.len() {
        if args[i] == "--fps" {
            if let Some(v) = args.get(i + 1).and_then(|s| s.parse::<u32>().ok()) {
                fps = v.clamp(1, 240);
            }
            i += 2;
        } else if matches!(
            args[i].as_str(),
            "--revelation" | "--revelation-size" | "--revelation-scale" | "--rev"
        ) {
            revelation_size = args
                .get(i + 1)
                .filter(|s| !s.starts_with("--"))
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(0.3);
            i += if args.get(i + 1).is_some_and(|s| !s.starts_with("--")) {
                2
            } else {
                1
            };
        } else if matches!(
            args[i].as_str(),
            "--hwaccel" | "--hardware-accel" | "--hardware-encoder"
        ) {
            let next = args.get(i + 1).filter(|s| !s.starts_with("--"));
            if let Some(encoder) = next {
                let lower = encoder.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "h264_nvenc" | "hevc_nvenc" | "h264_qsv" | "hevc_qsv" | "h264_amf" | "hevc_amf"
                ) {
                    hw_encoder = Some(lower);
                    i += 2;
                } else {
                    hw_encoder = Some("h264_nvenc".to_string());
                    i += 1;
                }
            } else {
                hw_encoder = Some("h264_nvenc".to_string());
                i += 1;
            }
        } else if args[i].starts_with("--") {
            i += 1;
        } else {
            positional.push(args[i].clone());
            i += 1;
        }
    }

    let chart_path = positional
        .iter()
        .find(|p| looks_like_chart_path(p))
        .cloned()
        .unwrap_or_default();

    let bgm_path = positional
        .iter()
        .find(|p| looks_like_audio_path(p))
        .cloned()
        .unwrap_or_default();

    let output_path = positional
        .iter()
        .find(|p| {
            matches!(
                Path::new(p)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.to_ascii_lowercase())
                    .as_deref(),
                Some("mp4" | "mkv" | "mov" | "webm")
            )
        })
        .cloned()
        .unwrap_or_else(|| "render_output.mp4".to_string());

    Some(RenderExportConfig {
        chart_path,
        bgm_path,
        output_path,
        fps,
        revelation_size,
        hw_encoder,
    })
}

fn is_render_export_mode() -> bool {
    parse_render_export_config().is_some()
}

struct AppState {
    chart: Option<Chart>,
    chart_path: String,
    bgm_path: String,
    pipeline: Option<RenderPipeline>,
    settings: RenderSettings,
    audio: AudioController,
    is_playing: bool,
    start_time: f64,
    fps_counter: u32,
    fps_time: f64,
    current_fps: u32,
    message: String,
    message_time: f64,
    audio_initialized: bool,
    font: Option<Font>,
    window_positioned: bool,
}

impl AppState {
    fn new() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let chart_path = if args.len() > 1 { args[1].clone() } else { String::new() };
        let bgm_path = if args.len() > 2 { args[2].clone() } else { String::new() };

        Self {
            chart: None,
            chart_path,
            bgm_path,
            pipeline: None,
            settings: RenderSettings::default(),
            audio: AudioController::new(),
            is_playing: false,
            start_time: 0.0,
            fps_counter: 0,
            fps_time: 0.0,
            current_fps: 0,
            message: String::new(),
            message_time: 0.0,
            audio_initialized: false,
            font: None,
            window_positioned: false,
        }
    }

    fn set_message(&mut self, msg: String) {
        self.message = msg;
        self.message_time = get_time();
    }

    fn select_chart_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("选择谱面 JSON 文件")
            .add_filter("Rizline Chart", &["json"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            let path = path.to_string_lossy().to_string();
            self.load_chart(&path);
        }
    }

    fn select_audio_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("选择音频文件")
            .add_filter("Audio", &["wav", "ogg", "mp3", "flac"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.bgm_path = path.to_string_lossy().to_string();
            self.audio = AudioController::new();
            self.audio_initialized = false;
            self.set_message(format!("Audio selected: {}", self.bgm_path));
        }
    }

    fn select_chart_and_audio_files(&mut self) {
        self.select_chart_file();
        self.select_audio_file();
    }

    fn init_audio(&mut self) {
        if self.audio_initialized { return; }
        match self.audio.init(&self.bgm_path) {
            Ok(()) => {
                log::info!("Audio initialized successfully");
                self.audio_initialized = true;
            }
            Err(e) => {
                log::warn!("Audio init failed (will continue without audio): {}", e);
                self.audio_initialized = true;
            }
        }
    }

    fn load_chart(&mut self, path: &str) {
        if path.is_empty() {
            self.set_message("No chart selected".to_string());
            return;
        }

        self.chart_path = path.to_string();
        match std::fs::read_to_string(path) {
            Ok(json) => match Chart::from_json(&json) {
                Ok(chart) => {
                    self.chart = Some(chart);
                    let note_count = self.chart.as_ref().map(|c| {
                        c.lines.iter().map(|l| l.notes.len()).sum()
                    }).unwrap_or(0);
                    self.set_message(format!("OK! Lines: {} | Notes: {}", 
                        self.chart.as_ref().unwrap().lines.len(), note_count));
                }
                Err(e) => self.set_message(format!("Parse error: {}", e)),
            },
            Err(e) => self.set_message(format!("Read error: {}", e)),
        }
    }

    fn start_play(&mut self) {
        if self.chart.is_none() {
            self.set_message("Load chart first".to_string());
            return;
        }
        self.init_audio();

        if let Some(ref chart) = self.chart {
            self.pipeline = Some(RenderPipeline::new(chart));
            if let Some(ref mut pipeline) = self.pipeline {
                for canvas in pipeline.canvases.iter_mut() {
                    canvas.init_fp(chart);
                }
            }
        }
        self.is_playing = true;
        self.start_time = get_time();
        self.audio.play_bgm();
        self.set_message("Playing...".to_string());
    }

    fn stop_play(&mut self) {
        self.is_playing = false;
        self.pipeline = None;
        self.audio.stop();
        self.set_message("Stopped".to_string());
    }

    fn update_input(&mut self) {
        if is_key_pressed(KeyCode::O) {
            self.select_chart_and_audio_files();
        }
        if is_key_pressed(KeyCode::C) {
            self.select_chart_file();
        }
        if is_key_pressed(KeyCode::B) {
            self.select_audio_file();
        }
        if is_key_pressed(KeyCode::L) {
            if self.chart_path.is_empty() {
                self.select_chart_file();
            } else {
                let path = self.chart_path.clone();
                self.load_chart(&path);
            }
        }
        if is_key_pressed(KeyCode::Up) {
            self.settings.speed_value = (self.settings.speed_value + 1).min(20);
            self.set_message(format!("Speed: {}", self.settings.speed_value));
        }
        if is_key_pressed(KeyCode::Down) {
            self.settings.speed_value = (self.settings.speed_value - 1).max(3);
            self.set_message(format!("Speed: {}", self.settings.speed_value));
        }
        if is_key_pressed(KeyCode::R) {
            self.settings.revelation_size = if self.settings.revelation_size < 1.0 { 1.0 } else { 0.3 };
            self.set_message(format!("Revelation: {:.1}", self.settings.revelation_size));
        }
        if is_key_pressed(KeyCode::D) {
            self.settings.show_debug = !self.settings.show_debug;
            self.set_message(format!("Debug: {}", self.settings.show_debug));
        }
        if is_key_pressed(KeyCode::Escape) {
            self.stop_play();
        }
        if is_key_pressed(KeyCode::Space) {
            if self.chart.is_some() {
                if self.is_playing { self.stop_play(); } else { self.start_play(); }
            }
        }
        if is_key_pressed(KeyCode::PageUp) {
            self.audio.set_bgm_volume(0.8);
            self.set_message("BGM Vol: 80%".to_string());
        }
        if is_key_pressed(KeyCode::PageDown) {
            self.audio.set_bgm_volume(0.3);
            self.set_message("BGM Vol: 30%".to_string());
        }
    }

    fn get_current_timer(&self) -> f64 {
        if !self.is_playing { return 0.0; }
        get_time() - self.start_time
    }

    #[cfg(target_os = "windows")]
    fn center_window_on_windows(&mut self) {
        if self.window_positioned {
            return;
        }

        use macroquad::miniquad::window::set_window_position;
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
        };

        unsafe {
            let mut work = RECT::default();
            let ok = SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work as *mut _ as *mut std::ffi::c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .is_ok();

            if !ok {
                self.window_positioned = true;
                return;
            }

            // Use the requested client size plus a conservative frame/title-bar allowance.
            // miniquad's set_window_position takes client-area coordinates and adjusts the
            // native outer rect internally, so this keeps the window visually centered while
            // avoiding the Windows work area occupied by the taskbar.
            let outer_w = WINDOW_W as i32 + 16;
            let outer_h = WINDOW_H as i32 + 39;
            let work_w = work.right - work.left;
            let work_h = work.bottom - work.top;

            let x = work.left + (work_w - outer_w).max(0) / 2;
            let mut y = work.top + (work_h - outer_h).max(0) / 2;

            let max_y = work.bottom - outer_h;
            if y > max_y {
                y = max_y;
            }
            if y < work.top {
                y = work.top;
            }

            set_window_position(x.max(0) as u32, y.max(0) as u32);
            self.window_positioned = true;
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn center_window_on_windows(&mut self) {
        self.window_positioned = true;
    }

    fn render_chart_frame(&mut self, timer: f64, with_audio_recovery: bool) {
        self.render_chart_frame_internal(timer, with_audio_recovery, None);
    }

    fn render_chart_frame_to_target(&mut self, timer: f64, target: &RenderTarget) {
        self.render_chart_frame_internal(timer, false, Some(target.clone()));
    }

    fn render_chart_frame_internal(
        &mut self,
        timer: f64,
        with_audio_recovery: bool,
        target: Option<RenderTarget>,
    ) {
        let screen_w = CANVAS_WIDTH;   // 1080
        let screen_h = CANVAS_HEIGHT;  // 1920

        // Camera setup to match original Canvas coordinate system:
        // Original: ctx.translate(cvs.width/2, cvs.height/2 + 200*(cvs.height/640))
        // Canvas Y-down: future notes above judge line, past notes below
        // macroquad Y-up: need to flip via negative zoom.y
        {
            let offset_y = 160.0 * (screen_h / 640.0); // 480
            let left = -screen_w / 2.0;       // -540
            let top = -screen_h / 2.0 - offset_y; // -1560

            let render_to_texture = target.is_some();
            let mut camera = Camera2D::from_display_rect(Rect::new(left, top, screen_w, screen_h));
            // Macroquad applies a different internal Y inversion when rendering to a RenderTarget.
            // Screen camera needs this extra flip; RenderTarget camera does not.
            if !render_to_texture {
                camera.zoom.y = -camera.zoom.y;
            }
            camera.render_target = target;

            set_camera(&camera);
            clear_background(BLACK);

            if let (Some(ref chart), Some(ref mut pipeline)) = (&self.chart, &mut self.pipeline) {
                let tick = seconds_to_tick(timer, chart);
                if with_audio_recovery {
                    self.audio.recover_if_needed();
                }

                for canvas in pipeline.canvases.iter_mut() {
                    canvas.update(tick, chart, &self.settings);
                }
                pipeline.draw_background(chart, tick);
                pipeline.draw_lines(chart, tick, &self.settings, self.font.as_ref());
                pipeline.draw_notes(chart, tick, &self.settings, &mut self.audio, self.font.as_ref());
                pipeline.draw_revelation_info(chart, tick, &self.settings, self.font.as_ref());
            }

            set_default_camera();
        }
    }

    fn render(&mut self) {
        self.center_window_on_windows();

        let window_w = screen_width();
        let window_h = screen_height();

        if self.is_playing {
            self.render_chart_frame(self.get_current_timer(), true);
        } else {
            clear_background(BLACK);
            if let (Some(ref chart), Some(ref pipeline)) = (&self.chart, &self.pipeline) {
                let screen_w = CANVAS_WIDTH;
                let screen_h = CANVAS_HEIGHT;
                let offset_y = 160.0 * (screen_h / 640.0);
                let left = -screen_w / 2.0;
                let top = -screen_h / 2.0 - offset_y;
                let mut camera = Camera2D::from_display_rect(Rect::new(left, top, screen_w, screen_h));
                camera.zoom.y = -camera.zoom.y;
                set_camera(&camera);
                pipeline.draw_background(chart, 0.0);
                set_default_camera();
            }
        }

        // UI layer (screen-space coordinates with origin at top-left)
        self.render_ui(window_w, window_h);
    }

    fn draw_text_with_font(&self, text: &str, x: f32, y: f32, size: f32, color: Color) {
        // Draw a strong outline + soft shadow so UI text remains readable on any theme background.
        let outline = (size * 0.08).max(1.5);
        let alpha = color.a.clamp(0.0, 1.0);
        let shadow = Color::new(0.0, 0.0, 0.0, 0.65 * alpha);
        let stroke = Color::new(0.0, 0.0, 0.0, 0.95 * alpha);

        let draw_once = |dx: f32, dy: f32, c: Color| {
            if let Some(ref font) = self.font {
                draw_text_ex(text, x + dx, y + dy, macroquad::text::TextParams {
                    font: Some(font),
                    font_size: size as u16,
                    font_scale: 1.0,
                    color: c,
                    ..Default::default()
                });
            } else {
                draw_text(text, x + dx, y + dy, size, c);
            }
        };

        draw_once(outline * 1.6, outline * 1.6, shadow);
        draw_once(-outline, 0.0, stroke);
        draw_once(outline, 0.0, stroke);
        draw_once(0.0, -outline, stroke);
        draw_once(0.0, outline, stroke);
        draw_once(-outline * 0.7, -outline * 0.7, stroke);
        draw_once(outline * 0.7, -outline * 0.7, stroke);
        draw_once(-outline * 0.7, outline * 0.7, stroke);
        draw_once(outline * 0.7, outline * 0.7, stroke);
        draw_once(0.0, 0.0, color);
    }

    fn measure_text_with_font(&self, text: &str, size: f32) -> f32 {
        if let Some(ref font) = self.font {
            measure_text(text, Some(font), size as u16, 1.0).width
        } else {
            measure_text(text, None, size as u16, 1.0).width
        }
    }

    fn render_ui(&mut self, window_w: f32, window_h: f32) {
        self.fps_counter += 1;
        let current_time = get_time();
        if current_time - self.fps_time >= 1.0 {
            self.current_fps = self.fps_counter;
            self.fps_counter = 0;
            self.fps_time = current_time;
        }

        let mut y_pos = 20.0f32;
        let line_height = 25.0f32;

        // Revelation mode has its own chart/debug overlay in world space.
        // Hide the normal top-left screen-space status UI to avoid overlap.
        if self.settings.revelation_size == 1.0 {
            self.draw_text_with_font(
                &format!("FPS: {}", self.current_fps), 10.0, y_pos, 20.0, WHITE
            );
        y_pos += line_height;

        let status = if self.is_playing { "Playing..." }
            else if self.chart.is_some() { "Loaded (Space to play)" }
            else { "No chart (O/C to select)" };
        self.draw_text_with_font(status, 10.0, y_pos, 20.0, YELLOW);
        y_pos += line_height;

        if self.audio_initialized {
            let pos = self.audio.get_bgm_position();
            let dur = self.audio.get_bgm_duration();
            let pos_min = (pos / 60.0).floor() as u32;
            let pos_sec = (pos % 60.0) as u32;
            let dur_min = (dur / 60.0).floor() as u32;
            let dur_sec = (dur % 60.0) as u32;
            let audio_str = format!("Audio: {:02}:{:02} / {:02}:{:02}", pos_min, pos_sec, dur_min, dur_sec);
            self.draw_text_with_font(&audio_str, 10.0, y_pos, 16.0, Color::new(0.0, 0.8, 0.4, 1.0));
            y_pos += line_height;

            if dur > 0.0 {
                let bar_w = 200.0f32;
                let bar_h = 8.0f32;
                let bar_x = 10.0f32;
                let progress = (pos / dur).min(1.0).max(0.0) as f32;
                draw_rectangle(bar_x, y_pos - 6.0, bar_w, bar_h, GRAY);
                draw_rectangle(bar_x, y_pos - 6.0, bar_w * progress, bar_h, Color::new(0.0, 0.8, 0.4, 1.0));
                draw_rectangle_lines(bar_x, y_pos - 6.0, bar_w, bar_h, 1.0, WHITE);
                y_pos += 20.0;
            }
        }

        self.draw_text_with_font(
            &format!("Speed: {} | Revel: {:.1} | Debug: {}", 
                self.settings.speed_value, 
                self.settings.revelation_size,
                self.settings.show_debug),
            10.0, y_pos, 16.0, LIGHTGRAY,
        );
        y_pos += line_height;

            if let Some(ref chart) = self.chart {
                let note_count: usize = chart.lines.iter().map(|l| l.notes.len()).sum();
                self.draw_text_with_font(
                    &format!("Chart: {} | Lines: {} | Notes: {}", 
                        self.chart_path, chart.lines.len(), note_count),
                    10.0, y_pos, 14.0, GRAY,
                );
            }
        }

        if !self.message.is_empty() && current_time - self.message_time < 3.0 {
            let alpha = ((3.0 - (current_time - self.message_time)) / 3.0).min(1.0) as f32;
            let alpha_color = Color::new(1.0, 1.0, 1.0, alpha);
            let msg_width = self.measure_text_with_font(&self.message, 20.0);
            self.draw_text_with_font(&self.message, window_w / 2.0 - msg_width / 2.0, window_h - 40.0, 20.0, alpha_color);
        }

        let hints = [
            "SPACE: Play/Stop",
            "Up/Down: Speed",
            "R: Revelation",
            "D: Debug",
            "O: Open chart+audio",
            "C/B: Chart/Audio",
            "L: Reload",
            "PgUp/PgDn: Vol",
            "ESC: Stop",
        ];
        let hint_alpha = if self.is_playing {
            let timer = self.get_current_timer();
            if timer <= 1.0 {
                1.0
            } else {
                let fade_duration = 0.8;
                let t = ((timer - 1.0) / fade_duration).clamp(0.0, 1.0) as f32;
                let ease_out_cubic = 1.0 - (1.0 - t).powi(3);
                1.0 - ease_out_cubic
            }
        } else {
            1.0
        };

        if hint_alpha > 0.01 {
            let hint_start_y = window_h - 185.0;
            let hint_color = Color::new(GRAY.r, GRAY.g, GRAY.b, GRAY.a * hint_alpha);
            for (i, hint) in hints.iter().enumerate() {
                self.draw_text_with_font(hint, 10.0, hint_start_y + i as f32 * 20.0, 13.0, hint_color);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn find_renderer_window() -> windows::Win32::Foundation::HWND {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    let title: Vec<u16> = OsStr::new("RE:CH-RZL-RUST Renderer")
        .encode_wide()
        .chain(Some(0))
        .collect();

    unsafe { FindWindowW(None, PCWSTR(title.as_ptr())).unwrap_or(HWND::default()) }
}

#[cfg(target_os = "windows")]
fn hide_render_window() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    let hwnd = find_renderer_window();
    if hwnd != HWND::default() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn hide_render_window() {}

#[cfg(target_os = "windows")]
struct TaskbarProgress {
    taskbar: Option<windows::Win32::UI::Shell::ITaskbarList3>,
    hwnd: windows::Win32::Foundation::HWND,
}

#[cfg(target_os = "windows")]
impl TaskbarProgress {
    fn new() -> Self {
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let taskbar = CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok();
            if let Some(tb) = &taskbar {
                let _ = tb.HrInit();
            }
            // Attach taskbar progress to the visible Macroquad renderer/progress window.
            // If it is not found yet, fall back to the console window.
            let mut hwnd = find_renderer_window();
            if hwnd == windows::Win32::Foundation::HWND::default() {
                hwnd = windows::Win32::System::Console::GetConsoleWindow();
            }
            Self { taskbar, hwnd }
        }
    }

    fn set(&self, done: u64, total: u64) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Shell::TBPF_NORMAL;

        if self.hwnd == HWND::default() {
            return;
        }
        if let Some(tb) = &self.taskbar {
            unsafe {
                let _ = tb.SetProgressState(self.hwnd, TBPF_NORMAL);
                let _ = tb.SetProgressValue(self.hwnd, done, total.max(1));
            }
        }
    }

    fn clear(&self) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Shell::TBPF_NOPROGRESS;

        if self.hwnd == HWND::default() {
            return;
        }
        if let Some(tb) = &self.taskbar {
            unsafe {
                let _ = tb.SetProgressState(self.hwnd, TBPF_NOPROGRESS);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
struct TaskbarProgress;

#[cfg(not(target_os = "windows"))]
impl TaskbarProgress {
    fn new() -> Self { Self }
    fn set(&self, _done: u64, _total: u64) {}
    fn clear(&self) {}
}

#[cfg(target_os = "windows")]
struct Win32ProgressWindow {
    dialog: Option<windows::Win32::UI::Shell::IProgressDialog>,
    started_at: Instant,
}

#[cfg(target_os = "windows")]
impl Win32ProgressWindow {
    fn wide(text: &str) -> Vec<u16> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        OsStr::new(text).encode_wide().chain(Some(0)).collect()
    }

    fn format_duration(seconds: f64) -> String {
        if !seconds.is_finite() || seconds <= 0.0 {
            return "0秒".to_string();
        }

        let mut total_seconds = seconds.round() as u64;
        let days = total_seconds / 86_400;
        total_seconds %= 86_400;
        let hours = total_seconds / 3_600;
        total_seconds %= 3_600;
        let minutes = total_seconds / 60;
        let secs = total_seconds % 60;

        if days > 0 {
            format!("{}天{}时{}分{}秒", days, hours, minutes, secs)
        } else if hours > 0 {
            format!("{}时{}分{}秒", hours, minutes, secs)
        } else if minutes > 0 {
            format!("{}分{}秒", minutes, secs)
        } else {
            format!("{}秒", secs)
        }
    }

    fn new(total: u32, output_path: &str) -> Self {
        use windows::core::{w, PCWSTR};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Shell::{
            IProgressDialog, CLSID_ProgressDialog, PDTIMER_RESET, PROGDLG_NOMINIMIZE,
            PROGDLG_NORMAL,
        };

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let dialog =
                CoCreateInstance::<_, IProgressDialog>(&CLSID_ProgressDialog, None, CLSCTX_INPROC_SERVER).ok();

            if let Some(dlg) = &dialog {
                let output = Self::wide(output_path);
                let _ = dlg.SetTitle(w!("正在渲染"));
                let _ = dlg.SetCancelMsg(w!("正在停止渲染，请稍候..."), None);
                let _ = dlg.SetLine(1, w!("正在渲染 RE:CH-RZL-RUST 视频"), false, None);
                let _ = dlg.SetLine(2, PCWSTR(output.as_ptr()), true, None);
                let _ = dlg.SetLine(3, w!("准备中..."), false, None);
                let _ = dlg.SetProgress64(0, total.max(1) as u64);
                let _ = dlg.Timer(PDTIMER_RESET, None);
                let _ = dlg.StartProgressDialog(
                    None,
                    None::<&windows::core::IUnknown>,
                    PROGDLG_NORMAL | PROGDLG_NOMINIMIZE,
                    None,
                );
            }

            Self {
                dialog,
                started_at: Instant::now(),
            }
        }
    }

    fn set_stage(&self, stage: &str, detail: &str, done: u64, total: u64) {
        use windows::core::PCWSTR;

        let Some(dlg) = &self.dialog else {
            return;
        };

        let pct = done as f64 * 100.0 / total.max(1) as f64;
        let elapsed = self.started_at.elapsed().as_secs_f64();
        let eta = if done > 0 && done < total {
            elapsed * (total - done) as f64 / done as f64
        } else {
            0.0
        };

        let elapsed_text = Self::format_duration(elapsed);
        let eta_text = Self::format_duration(eta);
        let stage = Self::wide(&format!("{} ({:.1}%)", stage, pct));
        let time_line = Self::wide(&format!("已用时 {} | 预计剩余 {}", elapsed_text, eta_text));
        let detail = Self::wide(detail);

        unsafe {
            let _ = dlg.SetLine(1, PCWSTR(stage.as_ptr()), false, None);
            let _ = dlg.SetLine(2, PCWSTR(time_line.as_ptr()), false, None);
            let _ = dlg.SetLine(3, PCWSTR(detail.as_ptr()), false, None);
            let _ = dlg.SetProgress64(done.min(total), total.max(1));
        }
    }

    fn set(&self, done: u32, total: u32, render_start: Instant) {
        use windows::core::PCWSTR;

        let Some(dlg) = &self.dialog else {
            return;
        };

        let pct = done as f64 * 100.0 / total.max(1) as f64;
        let elapsed = render_start.elapsed().as_secs_f64();
        let render_fps = if elapsed > 0.0 {
            done as f64 / elapsed
        } else {
            0.0
        };
        let eta = if done > 0 {
            elapsed * (total - done) as f64 / done as f64
        } else {
            0.0
        };

        let elapsed_text = Self::format_duration(elapsed);
        let eta_text = Self::format_duration(eta);
        let line1 = Self::wide(&format!(
            "正在渲染帧 {}/{} ({:.1}%) | {:.1} fps/s",
            done, total, pct, render_fps
        ));
        let line2 = Self::wide(&format!("已用时 {} | 预计剩余 {}", elapsed_text, eta_text));
        let line3 = Self::wide("正在写入视频帧到 FFmpeg");

        unsafe {
            let _ = dlg.SetProgress64(done.min(total) as u64, total.max(1) as u64);
            let _ = dlg.SetLine(1, PCWSTR(line1.as_ptr()), false, None);
            let _ = dlg.SetLine(2, PCWSTR(line2.as_ptr()), false, None);
            let _ = dlg.SetLine(3, PCWSTR(line3.as_ptr()), false, None);
        }
    }

    fn cancelled(&self) -> bool {
        self.dialog
            .as_ref()
            .map(|dlg| unsafe { dlg.HasUserCancelled().as_bool() })
            .unwrap_or(false)
    }

    fn close(&self) {
        if let Some(dlg) = &self.dialog {
            unsafe {
                let _ = dlg.StopProgressDialog();
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
struct Win32ProgressWindow;

#[cfg(not(target_os = "windows"))]
impl Win32ProgressWindow {
    fn new(_total: u32, _output_path: &str) -> Self { Self }
    fn set_stage(&self, _stage: &str, _detail: &str, _done: u64, _total: u64) {}
    fn set(&self, _done: u32, _total: u32, _render_start: Instant) {}
    fn cancelled(&self) -> bool { false }
    fn close(&self) {}
}

fn chart_duration_seconds(chart: &Chart) -> f64 {
    let mut max_tick = 0.0f64;

    for line in &chart.lines {
        for point in &line.line_points {
            max_tick = max_tick.max(point.time);
        }
        for note in &line.notes {
            max_tick = max_tick.max(note.time);
            if note.note_type == 2 {
                if let Some(end_tick) = note.other_informations.get(0) {
                    max_tick = max_tick.max(*end_tick + 0.5);
                }
            }
        }
    }

    for ct in &chart.challengeTimes {
        max_tick = max_tick.max(ct.end + ct.trans_time);
    }

    max_tick = max_tick.max(
        chart
            .cameraMove
            .scale_key_points
            .last()
            .map(|p| p.time)
            .unwrap_or(0.0),
    );
    max_tick = max_tick.max(
        chart
            .cameraMove
            .x_position_key_points
            .last()
            .map(|p| p.time)
            .unwrap_or(0.0),
    );

    (tick_to_seconds(max_tick, chart) + 2.0).max(1.0)
}

fn find_ffmpeg() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("ffmpeg.exe"),
        PathBuf::from("ch-rzl/ffmpeg.exe"),
        PathBuf::from("ffmpeg"),
    ];

    candidates.into_iter().find(|path| {
        if !path.exists() && path.components().count() > 1 {
            return false;
        }
        Command::new(path)
            .arg("-version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    })
}

fn require_ffmpeg() -> Result<PathBuf, String> {
    let ffmpeg = find_ffmpeg().ok_or_else(|| {
        "渲染前检查失败：没有找到 FFmpeg。请把 ffmpeg.exe 放到项目根目录，或放到 ch-rzl/ffmpeg.exe，或加入 PATH。".to_string()
    })?;
    println!("FFmpeg checked: {}", ffmpeg.display());
    Ok(ffmpeg)
}

fn sfx_path_for_note_type(note_type: u32) -> Option<(&'static str, f32)> {
    match note_type {
        // Match mixAudio.py: [hit, drag, hit, drag].
        // fresh is intentionally not used globally.
        1 | 3 => Some(("audio/drag.wav", 1.0)),
        _ => Some(("audio/hit.wav", 1.0)),
    }
}

fn collect_note_sfx_events(chart: &Chart) -> Vec<(u64, &'static str, f32)> {
    let mut events = Vec::new();

    for line in &chart.lines {
        for note in &line.notes {
            if let Some((path, volume)) = sfx_path_for_note_type(note.note_type) {
                if Path::new(path).exists() {
                    let delay_ms = (tick_to_seconds(note.time, chart).max(0.0) * 1000.0).round() as u64;
                    events.push((delay_ms, path, volume));
                }
            }
        }
    }

    events.sort_by_key(|event| event.0);
    events
}

#[derive(Clone)]
struct Pcm16Stereo {
    samples: Vec<i16>,
    sample_rate: u32,
}

fn read_wav_pcm16_stereo(path: &Path) -> Result<Pcm16Stereo, String> {
    let data = std::fs::read(path).map_err(|e| format!("Failed to read WAV {}: {e}", path.display()))?;

    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(format!("Invalid WAV file: {}", path.display()));
    }

    let mut offset = 12usize;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut audio_format = 0u16;
    let mut data_start = 0usize;
    let mut data_len = 0usize;

    while offset + 8 <= data.len() {
        let chunk_id = &data[offset..offset + 4];
        let chunk_size = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let chunk_start = offset + 8;
        let chunk_end = chunk_start.saturating_add(chunk_size).min(data.len());

        match chunk_id {
            b"fmt " if chunk_size >= 16 && chunk_end <= data.len() => {
                audio_format = u16::from_le_bytes(data[chunk_start..chunk_start + 2].try_into().unwrap());
                channels = u16::from_le_bytes(data[chunk_start + 2..chunk_start + 4].try_into().unwrap());
                sample_rate = u32::from_le_bytes(data[chunk_start + 4..chunk_start + 8].try_into().unwrap());
                bits_per_sample = u16::from_le_bytes(data[chunk_start + 14..chunk_start + 16].try_into().unwrap());
            }
            b"data" => {
                data_start = chunk_start;
                data_len = chunk_end.saturating_sub(chunk_start);
            }
            _ => {}
        }

        offset = chunk_start + chunk_size + (chunk_size % 2);
    }

    if audio_format != 1 || channels != 2 || bits_per_sample != 16 || sample_rate == 0 || data_len == 0 {
        return Err(format!(
            "Unsupported WAV format {}: format={}, channels={}, bits={}, sample_rate={}, data_len={}",
            path.display(),
            audio_format,
            channels,
            bits_per_sample,
            sample_rate,
            data_len
        ));
    }

    let mut samples = Vec::with_capacity(data_len / 2);
    for chunk in data[data_start..data_start + data_len].chunks_exact(2) {
        samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
    }

    Ok(Pcm16Stereo {
        samples,
        sample_rate,
    })
}

fn write_wav_pcm16_stereo(path: &Path, sample_rate: u32, samples: &[i16]) -> Result<(), String> {
    let file = std::fs::File::create(path)
        .map_err(|e| format!("Failed to create mixed WAV {}: {e}", path.display()))?;
    let mut file = BufWriter::with_capacity(1024 * 1024, file);

    let data_len = (samples.len() * 2) as u32;
    let riff_len = 36u32.saturating_add(data_len);
    let byte_rate = sample_rate * 2 * 16 / 8;
    let block_align = 2u16 * 16 / 8;

    file.write_all(b"RIFF").map_err(|e| e.to_string())?;
    file.write_all(&riff_len.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(b"WAVE").map_err(|e| e.to_string())?;
    file.write_all(b"fmt ").map_err(|e| e.to_string())?;
    file.write_all(&16u32.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&1u16.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&2u16.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&sample_rate.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&byte_rate.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&block_align.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&16u16.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(b"data").map_err(|e| e.to_string())?;
    file.write_all(&data_len.to_le_bytes()).map_err(|e| e.to_string())?;

    // Do not call write_all once per sample. A 146s stereo/44.1kHz WAV has
    // more than 12 million i16 samples, and per-sample writes make the app
    // look stuck after hit mixing is already done.
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;

    Ok(())
}

fn ffmpeg_decode_to_wav(
    ffmpeg: &Path,
    input: Option<&Path>,
    output: &Path,
    duration: Option<f64>,
    stage: &str,
    progress: &Win32ProgressWindow,
    taskbar: &TaskbarProgress,
    progress_from: u64,
    progress_to: u64,
) -> Result<(), String> {
    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-y")
        .arg("-hide_banner")
        .arg("-nostats");

    if let Some(input) = input {
        cmd.arg("-i").arg(input);
    } else {
        cmd.arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg("anullsrc=channel_layout=stereo:sample_rate=44100");
    }

    if let Some(duration) = duration {
        cmd.arg("-t").arg(format!("{:.3}", duration));
    }

    cmd.arg("-ac")
        .arg("2")
        .arg("-ar")
        .arg("44100")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg("-progress")
        .arg("pipe:1")
        .arg("-nostdin")
        .arg(output)
        .stdout(Stdio::piped())
        // Avoid a deadlock if FFmpeg writes enough diagnostics to fill stderr
        // while we are only reading the progress stream from stdout.
        .stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to start FFmpeg audio decode: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to read FFmpeg audio decode progress".to_string())?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let mut last_update = Instant::now();

    loop {
        if progress.cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            taskbar.clear();
            progress.close();
            return Err("用户取消了音频混合".to_string());
        }

        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("Failed to read FFmpeg audio decode progress: {e}"))?;

        if n == 0 {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| format!("Failed to wait audio decode FFmpeg: {e}"))?
            {
                if status.success() {
                    progress.set_stage(stage, "解码完成", progress_to, 100);
                    taskbar.set(progress_to, 100);
                    return Ok(());
                }

                let mut stderr_text = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    use std::io::Read;
                    let _ = stderr.read_to_string(&mut stderr_text);
                }
                return Err(format!("Failed to decode audio:\n{}", stderr_text));
            }

            std::thread::sleep(Duration::from_millis(20));
            continue;
        }

        if let (Some(total_duration), Some(raw)) = (duration, line.strip_prefix("out_time_ms=")) {
            if let Ok(out_time_us) = raw.trim().parse::<u64>() {
                let local = ((out_time_us as f64 / 1_000_000.0 / total_duration.max(0.001)) * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as u64;
                let done = progress_from + (progress_to - progress_from) * local / 100;

                if last_update.elapsed() >= Duration::from_millis(100) || done >= progress_to {
                    progress.set_stage(stage, &format!("解码进度 {}%", local), done, 100);
                    taskbar.set(done, 100);
                    last_update = Instant::now();
                }
            }
        }
    }
}

fn exe_dir_output_path(file_name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join(file_name)))
        .unwrap_or_else(|| PathBuf::from(file_name))
}

fn make_mixed_audio(
    ffmpeg: &Path,
    chart: &Chart,
    bgm_path: &str,
    duration: f64,
    progress: &Win32ProgressWindow,
    taskbar: &TaskbarProgress,
) -> Result<Option<PathBuf>, String> {
    let out = exe_dir_output_path("render_mixed_audio.wav");
    let bgm_wav = PathBuf::from("target").join("render_bgm_44100_s16.wav");
    let hit_wav = PathBuf::from("target").join("render_sfx_hit_44100_s16.wav");
    let drag_wav = PathBuf::from("target").join("render_sfx_drag_44100_s16.wav");
    let _ = std::fs::create_dir_all("target");

    let sfx_events = collect_note_sfx_events(chart);
    println!("Audio mix: {} hit sound events", sfx_events.len());

    progress.set_stage(
        "正在准备音频混合",
        &format!("打击音效 {} 个，将使用 Rust 快速离线混音", sfx_events.len()),
        0,
        100,
    );
    taskbar.set(0, 100);

    let bgm_input = if !bgm_path.is_empty() && Path::new(bgm_path).exists() {
        Some(Path::new(bgm_path))
    } else {
        None
    };

    let bgm_stage = bgm_input
        .and_then(|path| path.extension().and_then(|ext| ext.to_str()))
        .map(|ext| {
            if ext.eq_ignore_ascii_case("wav") {
                "正在标准化背景 WAV"
            } else {
                "正在转换背景音频为 WAV"
            }
        })
        .unwrap_or("正在生成静音 WAV");

    let bgm_detail = bgm_input
        .map(|path| {
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("wav"))
                .unwrap_or(false)
            {
                "统一转换为 44100Hz/16bit/双声道 WAV 以便离线混音".to_string()
            } else {
                format!(
                    "{} -> {}",
                    path.display(),
                    bgm_wav.display()
                )
            }
        })
        .unwrap_or_else(|| "未选择 BGM，生成静音 WAV 轨道".to_string());

    progress.set_stage(bgm_stage, &bgm_detail, 0, 100);
    taskbar.set(0, 100);

    ffmpeg_decode_to_wav(
        ffmpeg,
        bgm_input,
        &bgm_wav,
        Some(duration),
        bgm_stage,
        progress,
        taskbar,
        0,
        35,
    )?;

    let mut sfx_decode_done = 35u64;
    let mut sfx_cache: std::collections::HashMap<&'static str, Pcm16Stereo> = std::collections::HashMap::new();

    let unique_sfx = ["audio/hit.wav", "audio/drag.wav"];
    for path in unique_sfx {
        if !Path::new(path).exists() {
            continue;
        }

        let output = if path.contains("hit") {
            &hit_wav
        } else {
            &drag_wav
        };

        progress.set_stage(
            "正在解码打击音效",
            &format!("解码 {}", path),
            sfx_decode_done,
            100,
        );
        taskbar.set(sfx_decode_done, 100);

        ffmpeg_decode_to_wav(
            ffmpeg,
            Some(Path::new(path)),
            output,
            None,
            "正在解码打击音效",
            progress,
            taskbar,
            sfx_decode_done,
            (sfx_decode_done + 5).min(50),
        )?;

        sfx_cache.insert(path, read_wav_pcm16_stereo(output)?);
        sfx_decode_done = (sfx_decode_done + 5).min(50);
    }

    progress.set_stage(
        "正在读取背景音乐",
        "准备叠加打击音效",
        50,
        100,
    );
    taskbar.set(50, 100);

    let bgm = read_wav_pcm16_stereo(&bgm_wav)?;
    let sample_rate = bgm.sample_rate;
    let target_samples = ((duration * sample_rate as f64).ceil() as usize) * 2;
    let mut mix = vec![0f32; target_samples];

    for (i, sample) in bgm.samples.iter().take(target_samples).enumerate() {
        mix[i] = *sample as f32;
    }

    let total_events = sfx_events.len().max(1);
    let mut last_update = Instant::now();

    for (event_index, (delay_ms, path, volume)) in sfx_events.iter().enumerate() {
        if progress.cancelled() {
            taskbar.clear();
            progress.close();
            return Err("用户取消了音频混合".to_string());
        }

        let Some(sfx) = sfx_cache.get(path) else {
            continue;
        };

        let start_frame = ((*delay_ms as f64 / 1000.0) * sample_rate as f64).round() as usize;
        let start_sample = start_frame * 2;
        if start_sample >= mix.len() {
            continue;
        }

        let max_len = (mix.len() - start_sample).min(sfx.samples.len());
        for i in 0..max_len {
            mix[start_sample + i] += sfx.samples[i] as f32 * *volume;
        }

        if last_update.elapsed() >= Duration::from_millis(30) || event_index + 1 == sfx_events.len() {
            progress.set_stage(
                "正在混合背景音乐和打击音效",
                &format!("已混合 {}/{} 个打击音效", event_index + 1, sfx_events.len()),
                (event_index + 1) as u64,
                total_events as u64,
            );
            taskbar.set((event_index + 1) as u64, total_events as u64);
            last_update = Instant::now();
        }
    }

    progress.set_stage(
        "正在写出混合音频",
        &format!("保存 {}", out.display()),
        95,
        100,
    );
    taskbar.set(95, 100);

    let final_samples: Vec<i16> = mix
        .into_iter()
        .map(|sample| sample.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16)
        .collect();

    write_wav_pcm16_stereo(&out, sample_rate, &final_samples)?;

    progress.set_stage("音频混合完成", "准备开始渲染画面", 100, 100);
    taskbar.set(100, 100);

    println!("Saving mixed audio: {}", out.display());
    Ok(Some(out))
}

fn rgba_top_down_from_image(image: &Image) -> Result<Vec<u8>, String> {
    let width = image.width as usize;
    let height = image.height as usize;
    let row_len = width * 4;
    let expected_len = row_len * height;

    if image.bytes.len() != expected_len {
        return Err(format!(
            "RenderTarget image size mismatch: {}x{} => expected {} bytes, got {} bytes",
            width,
            height,
            expected_len,
            image.bytes.len()
        ));
    }

    // macroquad/miniquad reads texture pixels in OpenGL's bottom-up order.
    // FFmpeg rawvideo expects scanlines from top to bottom, so flip rows vertically.
    let mut out = vec![0u8; expected_len];
    for y in 0..height {
        let src_start = (height - 1 - y) * row_len;
        let dst_start = y * row_len;
        out[dst_start..dst_start + row_len]
            .copy_from_slice(&image.bytes[src_start..src_start + row_len]);
    }

    Ok(out)
}

fn spawn_ffmpeg_rawvideo(
    ffmpeg: &Path,
    fps: u32,
    audio_path: Option<&Path>,
    output_path: &str,
    hw_encoder: Option<&str>,
) -> Result<std::process::Child, String> {
    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-y")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-s")
        .arg(format!("{}x{}", CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32))
        .arg("-r")
        .arg(fps.to_string())
        .arg("-i")
        .arg("pipe:0");

    if let Some(audio) = audio_path {
        cmd.arg("-i").arg(audio);
    }

    cmd.arg("-c:v")
        .arg(hw_encoder.unwrap_or("libx264"))
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-r")
        .arg(fps.to_string());

    if audio_path.is_some() {
        cmd.arg("-c:a").arg("aac").arg("-shortest");
    }

    cmd.arg(output_path)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::null());

    if let Some(encoder) = hw_encoder {
        println!("Starting FFmpeg rawvideo pipe with hardware encoder: {encoder}");
    } else {
        println!("Starting FFmpeg rawvideo pipe with software encoder: libx264");
    }
    cmd.spawn().map_err(|e| format!("Failed to start FFmpeg: {e}"))
}

fn draw_export_progress_ui(
    app: &AppState,
    done: u32,
    total: u32,
    render_start: Instant,
    output_path: &str,
) {
    set_default_camera();
    clear_background(Color::new(0.06, 0.06, 0.08, 1.0));

    let pct = done as f32 / total.max(1) as f32;
    let elapsed = render_start.elapsed().as_secs_f32();
    let eta = if done > 0 {
        elapsed * (total - done) as f32 / done as f32
    } else {
        0.0
    };

    app.draw_text_with_font("RE:CH-RZL-RUST Renderer", 24.0, 36.0, 24.0, WHITE);
    app.draw_text_with_font(
        &format!("Output: {}", output_path),
        24.0,
        68.0,
        16.0,
        LIGHTGRAY,
    );
    let render_fps = if elapsed > 0.0 {
        done as f32 / elapsed
    } else {
        0.0
    };

    app.draw_text_with_font(
        &format!(
            "Frames: {}/{} ({:.1}%) | {:.1} fps/s | elapsed {:.1}s | ETA {:.1}s",
            done,
            total,
            pct * 100.0,
            render_fps,
            elapsed,
            eta,
        ),
        24.0,
        96.0,
        16.0,
        YELLOW,
    );

    let bar_x = 24.0;
    let bar_y = 124.0;
    let bar_w = screen_width() - 48.0;
    let bar_h = 22.0;
    draw_rectangle(bar_x, bar_y, bar_w, bar_h, Color::new(0.18, 0.18, 0.22, 1.0));
    draw_rectangle(
        bar_x,
        bar_y,
        bar_w * pct.clamp(0.0, 1.0),
        bar_h,
        Color::new(0.1, 0.75, 0.35, 1.0),
    );
    draw_rectangle_lines(bar_x, bar_y, bar_w, bar_h, 2.0, WHITE);
}

fn looks_like_chart_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
}

fn looks_like_audio_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("wav" | "ogg" | "mp3" | "flac" | "aac" | "m4a")
    )
}

async fn pick_render_chart_file(app: &AppState) -> Option<String> {
    clear_background(Color::new(0.06, 0.06, 0.08, 1.0));
    app.draw_text_with_font("RE:CH-RZL-RUST Renderer", 24.0, 42.0, 24.0, WHITE);
    app.draw_text_with_font("未传入谱面文件，正在打开文件选择框...", 24.0, 82.0, 18.0, YELLOW);
    app.draw_text_with_font("请选择一个 Rizline 谱面 JSON 文件", 24.0, 112.0, 16.0, LIGHTGRAY);
    next_frame().await;

    rfd::AsyncFileDialog::new()
        .set_title("选择要渲染的谱面 JSON 文件")
        .add_filter("Rizline Chart", &["json"])
        .add_filter("All files", &["*"])
        .pick_file()
        .await
        .map(|file| file.path().to_string_lossy().to_string())
}

async fn pick_render_audio_file(app: &AppState) -> Option<String> {
    clear_background(Color::new(0.06, 0.06, 0.08, 1.0));
    app.draw_text_with_font("RE:CH-RZL-RUST Renderer", 24.0, 42.0, 24.0, WHITE);
    app.draw_text_with_font("未传入音频文件，正在打开文件选择框...", 24.0, 82.0, 18.0, YELLOW);
    app.draw_text_with_font("可选择 BGM 音频；取消则只渲染打击音/静音轨", 24.0, 112.0, 16.0, LIGHTGRAY);
    next_frame().await;

    rfd::AsyncFileDialog::new()
        .set_title("选择要混入的音频文件（可取消）")
        .add_filter("Audio", &["wav", "ogg", "mp3", "flac", "aac", "m4a"])
        .add_filter("All files", &["*"])
        .pick_file()
        .await
        .map(|file| file.path().to_string_lossy().to_string())
}

async fn run_render_export(app: &mut AppState, mut config: RenderExportConfig) -> Result<(), String> {
    let total_start = Instant::now();

    if config.chart_path.is_empty() || !looks_like_chart_path(&config.chart_path) {
        if !config.chart_path.is_empty() && looks_like_audio_path(&config.chart_path) && config.bgm_path.is_empty() {
            config.bgm_path = config.chart_path.clone();
        }

        config.chart_path = pick_render_chart_file(app)
            .await
            .ok_or_else(|| "渲染已取消：没有选择谱面文件".to_string())?;
    }

    if config.bgm_path.is_empty() {
        config.bgm_path = pick_render_audio_file(app)
            .await
            .unwrap_or_default();
    }

    // Check FFmpeg after file selection so render mode without path can open the picker first.
    let ffmpeg = require_ffmpeg()?;

    println!("RE:CH-RZL-RUST offline renderer");
    println!("Chart : {}", config.chart_path);
    println!("Audio : {}", if config.bgm_path.is_empty() { "(none)" } else { &config.bgm_path });
    println!("Output: {}", config.output_path);
    println!("FPS   : {}", config.fps);
    println!(
        "Encoder: {}",
        config.hw_encoder.as_deref().unwrap_or("libx264 (software)")
    );
    println!(
        "Revelation: {}",
        if config.revelation_size == 1.0 {
            "off (1.0)".to_string()
        } else {
            format!("{}", config.revelation_size)
        }
    );

    app.chart_path = config.chart_path.clone();
    app.bgm_path = config.bgm_path.clone();
    app.settings.revelation_size = config.revelation_size;
    app.settings.recorder_watermark = true;
    app.load_chart(&config.chart_path);

    let chart = app
        .chart
        .as_ref()
        .ok_or_else(|| "Chart load failed".to_string())?
        .clone();

    app.pipeline = Some(RenderPipeline::new(&chart));
    if let Some(ref mut pipeline) = app.pipeline {
        for canvas in pipeline.canvases.iter_mut() {
            canvas.init_fp(&chart);
        }
    }

    // Do not initialize realtime audio during export. This keeps rendering deterministic and avoids
    // playing hit sounds while frames are generated. FFmpeg muxes the pre-rendered mixed WAV.
    app.audio = AudioController::new();

    let duration = chart_duration_seconds(&chart);
    let frame_count = (duration * config.fps as f64).ceil() as u32;
    println!("Rendering {:.2}s, {} frames", duration, frame_count);

    // Create the Win7-style progress dialog before audio mixing so both the
    // audio mix phase and the frame render phase are visible and cancellable.
    let taskbar = TaskbarProgress::new();
    let win32_progress = Win32ProgressWindow::new(100, &config.output_path);
    next_frame().await;
    hide_render_window();

    let mixed_audio = make_mixed_audio(&ffmpeg, &chart, &config.bgm_path, duration, &win32_progress, &taskbar)?;
    let mut ffmpeg_child = spawn_ffmpeg_rawvideo(
        &ffmpeg,
        config.fps,
        mixed_audio.as_deref(),
        &config.output_path,
        config.hw_encoder.as_deref(),
    )?;
    let mut ffmpeg_stdin = ffmpeg_child
        .stdin
        .take()
        .ok_or_else(|| "Failed to open FFmpeg stdin".to_string())?;

    // Use a fixed-size offscreen render target. Reading the actual screen buffer is clipped by the
    // visible monitor/window area on Windows, which caused partial frames when 1080x1920 exceeded
    // the display work area.
    let export_target = render_target(CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);
    export_target.texture.set_filter(FilterMode::Nearest);

    // Allow macroquad to create its GL context once. Actual video frames are rendered offscreen.
    // Progress is shown by the Windows Shell IProgressDialog (Win7 file-copy style).
    let render_start = Instant::now();
    win32_progress.set(0, frame_count, render_start);
    next_frame().await;
    for frame in 0..frame_count {
        if win32_progress.cancelled() {
            let _ = ffmpeg_child.kill();
            taskbar.clear();
            win32_progress.close();
            return Err("用户取消了画面渲染".to_string());
        }

        let timer = frame as f64 / config.fps as f64;
        app.render_chart_frame_to_target(timer, &export_target);

        let image = export_target.texture.get_texture_data();
        let frame_bytes = rgba_top_down_from_image(&image)?;
        ffmpeg_stdin
            .write_all(&frame_bytes)
            .map_err(|e| format!("Failed to pipe raw frame {} to FFmpeg: {e}", frame))?;

        let done = frame + 1;
        taskbar.set(done as u64, frame_count as u64);
        win32_progress.set(done, frame_count, render_start);

        if frame == 0 || done == frame_count || done % config.fps.max(1) == 0 {
            let pct = done as f64 * 100.0 / frame_count.max(1) as f64;
            let elapsed = render_start.elapsed().as_secs_f64();
            let render_fps = if elapsed > 0.0 {
                done as f64 / elapsed
            } else {
                0.0
            };
            println!(
                "Piped {}/{} frames ({:.1}%) | {:.1} fps/s | elapsed {:.2}s",
                done,
                frame_count,
                pct,
                render_fps,
                elapsed
            );
        }

        next_frame().await;
    }

    drop(ffmpeg_stdin);

    let encode_start = Instant::now();
    let output = ffmpeg_child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait FFmpeg: {e}"))?;
    taskbar.clear();
    win32_progress.close();

    if !output.status.success() {
        return Err(format!(
            "FFmpeg failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    println!("Encoding elapsed: {:.2}s", encode_start.elapsed().as_secs_f64());

    if let Some(path) = mixed_audio {
        println!("Mixed audio saved: {}", path.display());
    }

    println!("Render completed: {}", config.output_path);
    println!("Total render time: {:.2}s", total_start.elapsed().as_secs_f64());
    Ok(())
}

fn window_conf() -> Conf {
    let export_mode = is_render_export_mode();
    Conf {
        window_title: if export_mode {
            "RE:CH-RZL-RUST Renderer".to_string()
        } else {
            "RE:CH-RZL-RUST Player".to_string()
        },
        // Keep the default playback window below common Windows work-area height.
        // Export mode uses a compact visible progress window; actual video frames are rendered
        // offscreen at the full 1080x1920 canvas size.
        window_width: if export_mode { 620 } else { WINDOW_W as i32 },
        window_height: if export_mode { 180 } else { WINDOW_H as i32 },
        // Disable MSAA by default. The player currently draws many line segments per frame;
        // 4x MSAA costs a lot on integrated GPUs and can keep FPS low even after culling.
        sample_count: 1,
        high_dpi: false,
        window_resizable: false,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let export_config = parse_render_export_config();

    log::info!(
        "RE:CH-RZL-RUST {} starting...",
        if export_config.is_some() { "Renderer" } else { "Player" }
    );
    log::info!("Window: {}x{} (9:16)", WINDOW_W, WINDOW_H);
    log::info!("Canvas internal: {}x{}", CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);

    let mut app = AppState::new();
    if let Some(config) = &export_config {
        app.chart_path = config.chart_path.clone();
        app.bgm_path = config.bgm_path.clone();
    }

    log::info!("Chart path: {}", app.chart_path);
    if !app.bgm_path.is_empty() {
        log::info!("BGM path: {}", app.bgm_path);
    }

    // Load Chinese font
    let font_paths = [
        "rizline.ttf",
        "ch-rzl/fonts/rizline.ttf",
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/simsun.ttc",
    ];
    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = load_ttf_font_from_bytes(&data) {
                let mut chars = vec![];
                for c in (32u8..=127u8).map(|x| x as u32) {
                    chars.push(char::from_u32(c).unwrap());
                }
                for c in "谱面线条音符速度还原调试播放停止加载中成功失败请刷新音量信息FPS已加载未加载".chars() {
                    chars.push(c);
                }
                for c in "OK!/:RE:CH-RZL-RUST PLAYER RECORDER v0.1.0 by CHCAT1320CATPLAY".chars() {
                    chars.push(c);
                }
                for size in [13, 14, 16, 20, 24, 30, 36, 48, 60, 90] {
                    font.populate_font_cache(&chars, size);
                }
                app.font = Some(font);
                log::info!("Loaded font from: {}", path);
                break;
            }
        }
    }
    if app.font.is_none() {
        log::warn!("No custom font loaded");
    }

    if let Some(config) = export_config {
        if let Err(e) = run_render_export(&mut app, config).await {
            eprintln!("Render failed: {}", e);
        }
        return;
    }

    // Load chart from command line if provided. Otherwise use the file dialog shortcuts in the UI.
    let chart_path = app.chart_path.clone();
    if !chart_path.is_empty() {
        app.load_chart(&chart_path);
    } else {
        app.set_message("Press O to select chart and audio".to_string());
    }

    let mut auto_play_triggered = false;

    loop {
        next_frame().await;
        app.update_input();

        if !auto_play_triggered && app.chart.is_some() && !app.is_playing {
            app.start_play();
            auto_play_triggered = true;
            log::info!("Auto-play started");
        }

        app.render();
    }
}
