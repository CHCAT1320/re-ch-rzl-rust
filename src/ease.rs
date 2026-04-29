/// 缓动函数模块
/// 19个缓动函数，索引 0-18

/// linear
pub fn linear(x: f64) -> f64 {
    x
}

/// easeInQuad
pub fn ease_in_quad(x: f64) -> f64 {
    x * x
}

/// easeOutQuad
pub fn ease_out_quad(x: f64) -> f64 {
    1.0 - (1.0 - x) * (1.0 - x)
}

/// easeInOutQuad
pub fn ease_in_out_quad(x: f64) -> f64 {
    if x < 0.5 {
        2.0 * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(2) / 2.0
    }
}

/// easeInCubic
pub fn ease_in_cubic(x: f64) -> f64 {
    x * x * x
}

/// easeOutCubic
pub fn ease_out_cubic(x: f64) -> f64 {
    1.0 - (1.0 - x).powi(3)
}

/// easeInOutCubic
pub fn ease_in_out_cubic(x: f64) -> f64 {
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
    }
}

/// easeInQuart
pub fn ease_in_quart(x: f64) -> f64 {
    x * x * x * x
}

/// easeOutQuart
pub fn ease_out_quart(x: f64) -> f64 {
    1.0 - (1.0 - x).powi(4)
}

/// easeInOutQuart
pub fn ease_in_out_quart(x: f64) -> f64 {
    if x < 0.5 {
        8.0 * x * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(4) / 2.0
    }
}

/// easeInQuint
pub fn ease_in_quint(x: f64) -> f64 {
    x * x * x * x * x
}

/// easeOutQuint
pub fn ease_out_quint(x: f64) -> f64 {
    1.0 - (1.0 - x).powi(5)
}

/// easeInOutQuint
pub fn ease_in_out_quint(x: f64) -> f64 {
    if x < 0.5 {
        16.0 * x * x * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(5) / 2.0
    }
}

/// easeZero
pub fn ease_zero(_x: f64) -> f64 {
    0.0
}

/// easeOne
pub fn ease_one(_x: f64) -> f64 {
    1.0
}

/// easeInCirc
pub fn ease_in_circ(x: f64) -> f64 {
    1.0 - (1.0 - x * x).sqrt()
}

/// easeOutCirc
pub fn ease_out_circ(x: f64) -> f64 {
    (1.0 - (x - 1.0).powi(2)).sqrt()
}

/// easeOutSine
pub fn ease_out_sine(x: f64) -> f64 {
    (x * std::f64::consts::PI / 2.0).sin()
}

/// easeInSine
pub fn ease_in_sine(x: f64) -> f64 {
    1.0 - (x * std::f64::consts::PI / 2.0).cos()
}

/// 缓动函数数组（19个，索引0-18）
pub const EASE_FUNCS: [fn(f64) -> f64; 19] = [
    linear,          // 0: Linear
    ease_in_quad,    // 1: InQuad
    ease_out_quad,   // 2: OutQuad
    ease_in_out_quad, // 3: InOutQuad
    ease_in_cubic,   // 4: InCubic
    ease_out_cubic,  // 5: OutCubic
    ease_in_out_cubic, // 6: InOutCubic
    ease_in_quart,   // 7: InQuart
    ease_out_quart,  // 8: OutQuart
    ease_in_out_quart, // 9: InOutQuart
    ease_in_quint,   // 10: InQuint
    ease_out_quint,  // 11: OutQuint
    ease_in_out_quint, // 12: InOutQuint
    ease_zero,       // 13: Zero
    ease_one,        // 14: One
    ease_in_circ,    // 15: InCirc
    ease_out_circ,   // 16: OutCirc
    ease_out_sine,   // 17: OutSine
    ease_in_sine,    // 18: InSine
];

/// 线性插值
pub fn lerp(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}

/// 缓动执行函数
pub fn tween_execute(
    now_time: f64,
    start_time: f64,
    end_time: f64,
    start: f64,
    end: f64,
    ease_type: u32,
) -> f64 {
    let duration = end_time - start_time;
    let rdt = if duration <= 0.0 { 0.0 } else { (now_time - start_time) / duration };

    let ease_fn = if ease_type < EASE_FUNCS.len() as u32 {
        EASE_FUNCS[ease_type as usize]
    } else {
        EASE_FUNCS[0]
    };

    let t = ease_fn(rdt);
    lerp(start, end, t)
}
