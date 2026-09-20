# pmterm
pmterm is a beautiful, self-contained terminal (tui) pomodoro timer written in rust. it features an elegant minimalist interface, responsive layout blocks, keyboard-driven controls, and built-in, seamlessly looping ambient background sounds (like rain and fireplace) embedded directly into a single portable binary.

a terminal-based pomodoro timer with ambient sound support. written in rust with `ratatui`, `crossterm`, and `rodio`.

## installation

### prerequisites

- rust 1.81+ (install via [rustup](https://rustup.rs))
- linux: `libasound2-dev` (debian/ubuntu) or `alsa-lib-devel` (fedora)

### build from source

```sh
git clone <repo-url> pmterm
cd pmterm
cargo install --path .
```

the binary is self-contained — all audio files are compiled into the executable.

## usage

### controls

| key | action |
|---|---|
| `space` | pause / resume timer |
| `r` / `r` | reset current session |
| `s` / `s` | skip to next session |
| `1` | play rain ambient |
| `2` | play fireplace ambient |
| `0` | stop ambient sound |
| `↑` / `↓` | cycle ambient sounds |
| `m` / `m` | mute / unmute all audio |
| `q` / `q` / `esc` | quit |

### timer cycle

```
focus (25m) → short break (5m) → focus (25m) → short break (5m) →
focus (25m) → short break (5m) → focus (25m) → long break (15m) → …
```

after 4 focus sessions, a long break replaces the short break.

### audio

`rain.mp3` and `fireplace.mp3` are shipped in the repo and embedded into the binary at compile time via `include_bytes!`. supported formats depend on the `symphonia-all` feature — mp3, flac, ogg vorbis, wav, aac, and more.

## requirements

- **terminal**: any modern terminal emulator with true color support (gnome terminal, kitty, alacritty, wezterm, foot, etc.)
- **audio**: alsa (linux), pulseaudio, or pipewire
- **cargo**
