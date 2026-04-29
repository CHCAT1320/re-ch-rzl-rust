# RE:CH-RZL-RUST

RE:CH-RZL-RUST 是一个使用 Rust 重写的 Rizline 音游谱面播放器与离线渲染器。项目基于 macroquad 实现画面渲染，支持读取谱面 JSON、播放或选择背景音乐、显示音符与判定线动画，并可通过 FFmpeg 导出带音频的竖屏视频。

## 主要功能

- 谱面 JSON 解析与播放
- BGM 播放与打击音效播放
- 判定线、音符、Hold、Drag、打击特效渲染
- Challenge / Riztime 背景效果
- Revelation 谱面检查与展示模式
- 离线视频渲染导出
- 渲染进度窗口、任务栏进度与 fps/s 显示
- 背景音频自动转换或标准化为 WAV 后进行离线混音
- 支持软件编码与可选硬件编码
- 播放模式水印显示 PLAYER
- 渲染模式水印显示 RECORDER

## 技术栈

- Rust 2021
- macroquad：窗口、图形与文字渲染
- serde / serde_json：谱面 JSON 解析
- sasa / cpal / symphonia：音频播放
- rfd：文件选择窗口
- windows：Windows 进度窗口与任务栏进度
- FFmpeg：音频转换、视频编码与封装

## 项目结构

`	ext
.
├── Cargo.toml              # Rust 项目配置
├── src/
│   ├── main.rs             # 程序入口、播放 UI、离线渲染流程
│   ├── render.rs           # 谱面画面渲染管线
│   ├── chart.rs            # 谱面数据结构与 JSON 解析
│   ├── audio.rs            # 实时音频播放与音效控制
│   ├── ease.rs             # 缓动函数
│   └── time_conv.rs        # tick 与秒之间的转换
├── audio/                  # 打击音效资源
├── chart.*.json            # 示例谱面
└── *.wav                   # 示例音频
`

## 运行播放器

直接运行：

`cmd
cargo run
`

也可以传入谱面和音频：

`cmd
cargo run -- chart.RIP.eicateve.0.IN.json RIP.eicateve.0.wav
`

播放器快捷键：

| 按键 | 功能 |
| --- | --- |
| Space | 播放 / 停止 |
| O | 选择谱面和音频 |
| C | 选择谱面 |
| B | 选择音频 |
| L | 重新加载谱面 |
| Up / Down | 调整速度 |
| R | 切换 Revelation 显示比例 |
| D | 切换调试信息 |
| PageUp / PageDown | 调整音量 |
| Esc | 停止播放 |

## 离线渲染视频

渲染模式需要 FFmpeg。程序会依次查找：

1. 项目根目录的 fmpeg.exe
2. ch-rzl/ffmpeg.exe
3. 系统 PATH 中的 fmpeg

基本命令：

`cmd
cargo run -- --render chart.RIP.eicateve.0.IN.json RIP.eicateve.0.wav output.mp4
`

只传输出文件时，会弹出文件选择框选择谱面和音频：

`cmd
cargo run -- --render output.mp4
`

指定帧率：

`cmd
cargo run -- --render chart.RIP.eicateve.0.IN.json RIP.eicateve.0.wav output.mp4 --fps 60
`

启用 Revelation 渲染：

`cmd
cargo run -- --render chart.RIP.eicateve.0.IN.json RIP.eicateve.0.wav output.mp4 --rev 0.3
`

## 硬件编码

默认不启用硬件编码，使用软件编码：

`	ext
libx264
`

启用默认硬件编码器 h264_nvenc：

`cmd
cargo run -- --render chart.RIP.eicateve.0.IN.json RIP.eicateve.0.wav output.mp4 --hwaccel
`

指定硬件编码器：

`cmd
cargo run -- --render chart.RIP.eicateve.0.IN.json RIP.eicateve.0.wav output.mp4 --hwaccel h264_nvenc
cargo run -- --render chart.RIP.eicateve.0.IN.json RIP.eicateve.0.wav output.mp4 --hwaccel h264_qsv
cargo run -- --render chart.RIP.eicateve.0.IN.json RIP.eicateve.0.wav output.mp4 --hwaccel h264_amf
`

支持的编码器：

`	ext
h264_nvenc / hevc_nvenc
h264_qsv   / hevc_qsv
h264_amf   / hevc_amf
`

## 输出文件

- 渲染视频：由命令行传入的输出路径决定，默认 
ender_output.mp4
- 混合音频：默认保存为 exe 所在目录下的 
ender_mixed_audio.wav
  - 开发运行时通常为 	arget/debug/render_mixed_audio.wav
  - 发布运行时为 exe 同目录

## 构建

调试构建：

`cmd
cargo build
`

发布构建：

`cmd
cargo build --release
`

## Git 忽略说明

根目录 .gitignore 已屏蔽原网页项目目录：

`gitignore
/ch-rzl/
`

这表示 ch-rzl 文件夹及其内部所有内容不会被 Git 跟踪。

## 说明

本项目是对原网页音游ch-rzl播放器的 Rust 迁移版本，目标是提供更独立的本地播放和离线录制 / 渲染能力。原网页项目目录 ch-rzl/ 可作为参考资源目录保留，但不纳入当前 Rust 项目的 Git 跟踪。
