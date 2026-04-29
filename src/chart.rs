use serde::Deserialize;

/// 谱面顶层数据结构
#[derive(Deserialize, Debug, Clone)]
pub struct Chart {
    #[serde(rename = "fileVersion")]
    pub file_version: u32,
    #[serde(rename = "bPM")]
    pub base_bpm: f64,
    pub bpmShifts: Vec<BpmShift>,
    pub canvasMoves: Vec<CanvasMove>,
    pub lines: Vec<Line>,
    pub cameraMove: CameraMove,
    pub themes: Vec<Theme>,
    pub challengeTimes: Vec<ChallengeTime>,
}

/// BPM变化点
#[derive(Deserialize, Debug, Clone)]
pub struct BpmShift {
    pub time: f64,
    pub value: f64,
    #[serde(rename = "floorPosition")]
    pub floor_position: f64,
}

/// 画布移动数据
#[derive(Deserialize, Debug, Clone)]
pub struct CanvasMove {
    pub index: u32,
    #[serde(rename = "xPositionKeyPoints")]
    pub x_position_key_points: Vec<KeyPoint>,
    #[serde(rename = "speedKeyPoints")]
    pub speed_key_points: Vec<SpeedKeyPoint>,
}

/// 关键点（位置、缩放等）
#[derive(Deserialize, Debug, Clone)]
pub struct KeyPoint {
    pub time: f64,
    pub value: f64,
    #[serde(rename = "easeType")]
    pub ease_type: u32,
}

/// 速度关键点
#[derive(Deserialize, Debug, Clone)]
pub struct SpeedKeyPoint {
    pub time: f64,
    pub value: f64,
    #[serde(default)]
    pub fp: f64,
}

/// 线条数据
#[derive(Deserialize, Debug, Clone)]
pub struct Line {
    #[serde(rename = "linePoints")]
    pub line_points: Vec<LinePoint>,
    pub notes: Vec<Note>,
    #[serde(rename = "lineColor", default)]
    pub line_color: Vec<LineColor>,
    #[serde(rename = "judgeRingColor", default)]
    pub judge_ring_color: Vec<LineColor>,
}

/// 线条上的点
#[derive(Deserialize, Debug, Clone)]
pub struct LinePoint {
    pub time: f64,
    #[serde(rename = "xPosition")]
    pub x_position: f64,
    #[serde(rename = "canvasIndex")]
    pub canvas_index: u32,
    #[serde(rename = "easeType")]
    pub ease_type: u32,
    pub color: Color,
    #[serde(default)]
    pub fp: Option<f64>,
    #[serde(default, rename = "mixColor")]
    pub mix_color: Option<Color>,
}

/// 音符数据
#[derive(Deserialize, Debug, Clone)]
pub struct Note {
    pub time: f64,
    #[serde(rename = "type")]
    pub note_type: u32,
    #[serde(rename = "floorPosition", default)]
    pub floor_position: f64,
    #[serde(rename = "otherInformations", default)]
    pub other_informations: Vec<f64>,
}

/// 颜色 (RGBA, 每个通道 0-255)
#[derive(Deserialize, Debug, Clone, Copy)]
pub struct Color {
    pub r: u32,
    pub g: u32,
    pub b: u32,
    pub a: u32,
}

impl Color {
    /// 转换为 normalized RGBA (0.0-1.0)
    pub fn to_rgba(&self) -> (f32, f32, f32, f32) {
        (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        )
    }
}

/// 线条颜色时间段
#[derive(Deserialize, Debug, Clone)]
pub struct LineColor {
    pub time: f64,
    #[serde(rename = "startColor")]
    pub start_color: Color,
    #[serde(rename = "endColor")]
    pub end_color: Color,
}

/// 相机移动
#[derive(Deserialize, Debug, Clone)]
pub struct CameraMove {
    #[serde(rename = "scaleKeyPoints")]
    pub scale_key_points: Vec<KeyPoint>,
    #[serde(rename = "xPositionKeyPoints")]
    pub x_position_key_points: Vec<KeyPoint>,
}

/// 主题
#[derive(Deserialize, Debug, Clone)]
pub struct Theme {
    #[serde(rename = "colorsList")]
    pub colors_list: Vec<Color>,
}

/// Challenge时间段
#[derive(Deserialize, Debug, Clone)]
pub struct ChallengeTime {
    pub start: f64,
    pub end: f64,
    #[serde(rename = "transTime")]
    pub trans_time: f64,
}

impl Chart {
    /// 从JSON字符串加载谱面
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }
}

// ==================== 运行时数据结构 ====================

/// 运行时画布状态
pub struct CanvasState {
    pub index: u32,
    pub x_position_key_points: Vec<KeyPoint>,
    pub speed_key_points: Vec<SpeedKeyPointRuntime>,
    pub x: f64,
    pub fp: f64,
}

/// 运行时速度关键点（带预计算的fp）
#[derive(Debug, Clone)]
pub struct SpeedKeyPointRuntime {
    pub time: f64,
    pub value: f64,
    pub fp: f64,
}

/// 运行时音符
pub struct NoteRuntime {
    pub info: Note,
    pub line_info: Line,
    pub finded_points: (LinePoint, LinePoint),
    pub fp: f64,
    pub is_hit: bool,
    pub is_play_hit: bool,
}

/// 击中特效
pub struct HitEffect {
    pub x: f64,
    pub timer: f64,
    pub color: Color,
    pub is_bad: bool,
    pub size: f64,
    pub t: f64,
    pub block_count: u32,
    pub blocks_r: Vec<f64>,
    pub blocks_d: Vec<f64>,
    pub block_s: Vec<f64>,
}

impl HitEffect {
    pub fn new(tick: f64, x: f64, color: Color, is_bad: bool) -> Self {
        let block_count: u32 = 3; // simplified: fixed 3 blocks
        Self {
            x,
            timer: tick,
            color,
            is_bad,
            size: 0.0,
            t: 0.0,
            block_count,
            blocks_r: vec![0.0, 120.0, 240.0],
            blocks_d: vec![1.0, 1.0, 1.0],
            block_s: vec![15.0, 20.0, 10.0],
        }
    }
}
