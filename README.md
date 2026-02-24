# RLIAMP

`cliamp` 的 Rust 重写版本：终端音乐播放器，支持 MP3 / M4A，包含播放列表、10 段频谱可视化、10 段 EQ、音量/seek 控制、shuffle/repeat。

为避免中文/日文/韩文终端下的双宽字符错位，UI 默认使用 ASCII 安全字符渲染。

## 运行

```bash
cargo run -- track.mp3
cargo run -- *.mp3
```

## 构建

```bash
cargo build --release
./target-user/release/rliamp *.mp3
```

## 按键

| Key | Action |
|---|---|
| `Space` / `p` | 播放 / 暂停 |
| `Enter` | 播放当前选中曲目 |
| `s` | 停止 |
| `>` `.` | 下一首 |
| `<` `,` | 上一首 |
| `Left` `Right` | 后退/前进 5 秒 |
| `+` `-` | 音量加减 |
| `Tab` | 切换焦点（播放列表 / EQ） |
| `j` `k` / `Up` `Down` | 播放列表移动 / EQ 增减 |
| `h` `l` | EQ 光标左右 |
| `r` | 循环模式切换（Off / All / One） |
| `z` | 开关 Shuffle |
| `q` | 退出 |
