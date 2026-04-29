use crate::chart::*;

/// 将秒数转换为 tick
pub fn seconds_to_tick(seconds: f64, chart: &Chart) -> f64 {
    let bpm_shifts = &chart.bpmShifts;
    let base_bpm = chart.base_bpm;

    if bpm_shifts.is_empty() {
        return seconds / (60.0 / base_bpm);
    }

    let mut prev = &bpm_shifts[0];
    if seconds <= prev.floor_position {
        return seconds / (60.0 / (base_bpm * prev.value));
    }

    for i in 1..bpm_shifts.len() {
        let curr = &bpm_shifts[i];
        if seconds <= curr.floor_position {
            let tick_start = prev.time;
            let tick_end = curr.time;
            let time_start = prev.floor_position;
            let time_end = curr.floor_position;
            let ratio = (seconds - time_start) / (time_end - time_start);
            return tick_start + ratio * (tick_end - tick_start);
        }
        prev = curr;
    }

    let last = &bpm_shifts[bpm_shifts.len() - 1];
    let extra_seconds = seconds - last.floor_position;
    let extra_ticks = extra_seconds / (60.0 / (base_bpm * last.value));
    last.time + extra_ticks
}

/// 将 tick 转换为秒数
pub fn tick_to_seconds(tick: f64, chart: &Chart) -> f64 {
    let bpm_shifts = &chart.bpmShifts;
    let base_bpm = chart.base_bpm;

    if bpm_shifts.is_empty() {
        return tick * (60.0 / base_bpm);
    }

    let first = &bpm_shifts[0];
    if tick <= first.time {
        return tick * (60.0 / (base_bpm * first.value));
    }

    for i in 1..bpm_shifts.len() {
        let curr = &bpm_shifts[i];
        if tick <= curr.time {
            let prev = &bpm_shifts[i - 1];
            let ratio = (tick - prev.time) / (curr.time - prev.time);
            let sec_start = prev.floor_position;
            let sec_end = curr.floor_position;
            return sec_start + ratio * (sec_end - sec_start);
        }
    }

    let last = &bpm_shifts[bpm_shifts.len() - 1];
    let extra_ticks = tick - last.time;
    let extra_seconds = extra_ticks * (60.0 / (base_bpm * last.value));
    last.floor_position + extra_seconds
}

/// 根据 tick 从 event 列表中查找 value（二分查找 + 缓动插值）
pub fn find_value(tick: f64, events: &[KeyPoint]) -> f64 {
    use crate::ease::EASE_FUNCS;

    if events.len() == 1 {
        return if tick >= events[0].time { events[0].value } else { 0.0 };
    }

    let last_event = &events[events.len() - 1];
    if tick > last_event.time {
        return last_event.value;
    }

    let mut left: isize = 0;
    let mut right: isize = (events.len() - 1) as isize;
    let mut event1: Option<&KeyPoint> = None;
    let mut event2: Option<&KeyPoint> = None;

    while left <= right {
        let mid = ((left + right) / 2) as usize;
        let mid_event = &events[mid];

        if mid_event.time == tick {
            return mid_event.value;
        } else if mid_event.time < tick {
            event1 = Some(mid_event);
            left = mid as isize + 1;
        } else {
            event2 = Some(mid_event);
            right = mid as isize - 1;
        }
    }

    if let (Some(e1), Some(e2)) = (event1, event2) {
        let ease_type = if e1.ease_type < EASE_FUNCS.len() as u32 {
            e1.ease_type
        } else {
            0
        };
        let ease_fn = EASE_FUNCS[ease_type as usize];
        let ease_value = ease_fn((tick - e1.time) / (e2.time - e1.time));
        e1.value + (e2.value - e1.value) * ease_value
    } else {
        0.0
    }
}

/// 根据 tick 从速度事件列表中查找 speed value
pub fn find_speed_value(tick: f64, events: &[SpeedKeyPointRuntime], chart: &Chart) -> f64 {
    if events.is_empty() {
        return 0.0;
    }

    let target_time = tick_to_seconds(tick, chart);

    let mut processed: Vec<(f64, f64, f64)> = Vec::with_capacity(events.len());
    for e in events {
        let time_sec = tick_to_seconds(e.time, chart);
        processed.push((time_sec, e.fp, e.value));
    }
    processed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    if processed.len() == 1 {
        let e = &processed[0];
        if target_time >= e.0 {
            e.1 + (target_time - e.0) * e.2
        } else {
            0.0
        }
    } else if target_time > processed[processed.len() - 1].0 {
        let last = &processed[processed.len() - 1];
        last.1 + (target_time - last.0) * last.2
    } else {
        let mut left: isize = 0;
        let mut right: isize = (processed.len() - 1) as isize;
        let mut event1: Option<&(f64, f64, f64)> = None;
        let mut event2: Option<&(f64, f64, f64)> = None;

        while left <= right {
            let mid = ((left + right) / 2) as usize;
            let mid_event = &processed[mid];

            if mid_event.0 == target_time {
                return mid_event.1;
            } else if mid_event.0 < target_time {
                event1 = Some(mid_event);
                left = mid as isize + 1;
            } else {
                event2 = Some(mid_event);
                right = mid as isize - 1;
            }
        }

        if let (Some(e1), Some(_e2)) = (event1, event2) {
            e1.1 + (target_time - e1.0) * e1.2
        } else {
            0.0
        }
    }
}

/// 查找当前 tick 所在的 challenge time 索引
pub fn get_challenge_time_index(tick: f64, chart: &Chart) -> Option<usize> {
    for (i, ct) in chart.challengeTimes.iter().enumerate() {
        if tick >= ct.start && tick <= ct.end {
            return Some(i);
        }
    }
    None
}
