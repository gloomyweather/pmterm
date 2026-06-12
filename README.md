# pmterm
pmterm is a beautiful, self-contained terminal (TUI) Pomodoro timer written in Rust. It features an elegant minimalist interface, responsive layout blocks, keyboard-driven controls, and built-in, seamlessly looping ambient background sounds (like rain and fireplace) embedded directly into a single portable binary.

A terminal-based Pomodoro timer with ambient sound support. Written in Rust with `ratatui`, `crossterm`, and `rodio`.

## Installation

### Prerequisites

- Rust 1.81+ (install via [rustup](https://rustup.rs))
- Linux: `libasound2-dev` (Debian/Ubuntu) or `alsa-lib-devel` (Fedora)

### Build from source

```sh
git clone <repo-url> pmterm
cd pmterm
cargo install --path .
```

The binary is self-contained — all audio files are compiled into the executable.

## Usage

### Controls

| Key | Action |
|---|---|
| `Space` | Pause / Resume timer |
| `r` / `R` | Reset current session |
| `s` / `S` | Skip to next session |
| `1` | Play rain ambient |
| `2` | Play fireplace ambient |
| `0` | Stop ambient sound |
| `↑` / `↓` | Cycle ambient sounds |
| `m` / `M` | Mute / Unmute all audio |
| `q` / `Q` / `Esc` | Quit |

### Timer cycle

```
Focus (25m) → Short Break (5m) → Focus (25m) → Short Break (5m) →
Focus (25m) → Short Break (5m) → Focus (25m) → Long Break (15m) → …
```

After 4 focus sessions, a long break replaces the short break.

### Audio

`rain.mp3` and `fireplace.mp3` are shipped in the repo and embedded into the binary at compile time via `include_bytes!`. Supported formats depend on the `symphonia-all` feature — MP3, FLAC, OGG Vorbis, WAV, AAC, and more.

## Requirements

- **Terminal**: Any modern terminal emulator with true color support (GNOME Terminal, Kitty, Alacritty, WezTerm, foot, etc.)
- **Audio**: ALSA (Linux), PulseAudio, or PipeWire
- **Cargo**
