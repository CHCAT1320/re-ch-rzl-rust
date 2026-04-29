use anyhow::Result;
use sasa::{AudioClip, AudioManager, MusicParams, PlaySfxParams, Sfx};
use sasa::backend::cpal::{CpalBackend, CpalSettings};

/// Audio controller using sasa library for audio management
pub struct AudioController {
    manager: Option<AudioManager>,
    bgm: Option<sasa::Music>,
    bgm_duration: f64,
    hit_sfx: Option<Sfx>,
    drag_sfx: Option<Sfx>,
}

impl AudioController {
    pub fn new() -> Self {
        AudioController {
            manager: None,
            bgm: None,
            bgm_duration: 0.0,
            hit_sfx: None,
            drag_sfx: None,
        }
    }

    /// Initialize audio system with optional custom BGM path
    pub fn init(&mut self, custom_bgm_path: &str) -> Result<()> {
        let mut manager = AudioManager::new(CpalBackend::new(CpalSettings::default()))?;

        // Load hit sound effect
        if let Ok(data) = std::fs::read("audio/hit.wav") {
            if let Ok(clip) = AudioClip::new(data) {
                self.hit_sfx = Some(manager.create_sfx(clip, None)?);
                log::info!("Loaded audio/hit.wav");
            }
        }

        // Load drag sound effect
        if let Ok(data) = std::fs::read("audio/drag.wav") {
            if let Ok(clip) = AudioClip::new(data) {
                self.drag_sfx = Some(manager.create_sfx(clip, None)?);
                log::info!("Loaded audio/drag.wav");
            }
        }

        // Load background music
        let bgm_path = if !custom_bgm_path.is_empty() && std::path::Path::new(custom_bgm_path).exists() {
            custom_bgm_path.to_string()
        } else {
            String::new()
        };

        self.bgm_duration = 0.0;
        if !bgm_path.is_empty() {
            if let Ok(data) = std::fs::read(&bgm_path) {
                self.bgm_duration = estimate_audio_duration(&bgm_path, &data).unwrap_or(0.0);
                if let Ok(clip) = AudioClip::new(data) {
                    let music = manager.create_music(
                        clip,
                        MusicParams {
                            loop_mix_time: -1.0,
                            amplifier: 0.5,
                            playback_rate: 1.0,
                            command_buffer_size: 16,
                        },
                    )?;
                    self.bgm = Some(music);
                    log::info!("Loaded BGM: {} ({:.2}s)", bgm_path, self.bgm_duration);
                }
            }
        } else {
            log::warn!("No BGM file found");
        }

        self.manager = Some(manager);
        Ok(())
    }

    /// Play hit sound effect when a note is hit.
    /// note_type mapping follows mixAudio.py: 0=hit, 1=drag, 2=hit, 3=drag.
    /// fresh is intentionally not used globally.
    pub fn play_hit_sound(&mut self, note_type: u32) {
        match note_type {
            1 | 3 => {
                if let Some(ref mut sfx) = self.drag_sfx {
                    if let Err(e) = sfx.play(PlaySfxParams { amplifier: 1.0 }) {
                        log::warn!("Failed to play drag sound: {}", e);
                    }
                }
            }
            _ => {
                if let Some(ref mut sfx) = self.hit_sfx {
                    if let Err(e) = sfx.play(PlaySfxParams { amplifier: 1.0 }) {
                        log::warn!("Failed to play hit sound: {}", e);
                    }
                }
            }
        }
    }

    /// Play background music
    pub fn play_bgm(&mut self) {
        if let Some(ref mut music) = self.bgm {
            // First try to pause, then play from beginning
            let _ = music.pause();
            if let Err(e) = music.play() {
                log::warn!("Failed to play BGM: {}", e);
            }
        }
    }

    /// Pause background music
    pub fn pause_bgm(&mut self) {
        if let Some(ref mut music) = self.bgm {
            if let Err(e) = music.pause() {
                log::warn!("Failed to pause BGM: {}", e);
            }
        }
    }

    /// Check if BGM is available
    pub fn is_bgm_playing(&self) -> bool {
        self.bgm.is_some()
    }

    /// Stop (pause) all audio
    pub fn stop(&mut self) {
        if let Some(ref mut music) = self.bgm {
            let _ = music.pause();
        }
    }

    /// Set BGM volume
    pub fn set_bgm_volume(&mut self, volume: f32) {
        if let Some(ref mut music) = self.bgm {
            let _ = music.set_amplifier(volume.clamp(0.0, 1.0));
        }
    }

    /// Get current BGM playback position in seconds
    pub fn get_bgm_position(&mut self) -> f64 {
        if let Some(ref mut music) = self.bgm {
            return music.position() as f64;
        }
        0.0
    }

    /// Get BGM duration in seconds.
    pub fn get_bgm_duration(&self) -> f64 {
        self.bgm_duration
    }

    /// Recover audio backend if it was broken
    pub fn recover_if_needed(&mut self) {
        if let Some(ref mut manager) = self.manager {
            if let Err(e) = manager.recover_if_needed() {
                log::warn!("Audio recovery failed: {}", e);
            }
        }
    }
}

fn estimate_audio_duration(path: &str, data: &[u8]) -> Option<f64> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".wav") {
        estimate_wav_duration(data)
    } else {
        // sasa does not expose Music duration. For non-WAV files keep duration unknown.
        None
    }
}

fn estimate_wav_duration(data: &[u8]) -> Option<f64> {
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }

    let mut offset = 12usize;
    let mut sample_rate = 0u32;
    let mut byte_rate = 0u32;
    let mut block_align = 0u16;
    let mut bits_per_sample = 0u16;
    let mut data_size = 0u32;

    while offset + 8 <= data.len() {
        let chunk_id = &data[offset..offset + 4];
        let chunk_size = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().ok()?) as usize;
        let chunk_start = offset + 8;
        let chunk_end = chunk_start.saturating_add(chunk_size).min(data.len());

        match chunk_id {
            b"fmt " if chunk_size >= 16 && chunk_end <= data.len() => {
                sample_rate = u32::from_le_bytes(data[chunk_start + 4..chunk_start + 8].try_into().ok()?);
                byte_rate = u32::from_le_bytes(data[chunk_start + 8..chunk_start + 12].try_into().ok()?);
                block_align = u16::from_le_bytes(data[chunk_start + 12..chunk_start + 14].try_into().ok()?);
                bits_per_sample = u16::from_le_bytes(data[chunk_start + 14..chunk_start + 16].try_into().ok()?);
            }
            b"data" => {
                data_size = chunk_size as u32;
            }
            _ => {}
        }

        offset = chunk_start + chunk_size + (chunk_size % 2);
    }

    if data_size == 0 {
        return None;
    }

    if byte_rate > 0 {
        Some(data_size as f64 / byte_rate as f64)
    } else if sample_rate > 0 && block_align > 0 {
        Some(data_size as f64 / block_align as f64 / sample_rate as f64)
    } else if sample_rate > 0 && bits_per_sample > 0 {
        Some(data_size as f64 * 8.0 / bits_per_sample as f64 / sample_rate as f64)
    } else {
        None
    }
}

impl Default for AudioController {
    fn default() -> Self {
        Self::new()
    }
}
