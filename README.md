# mdspeak / mdhear

Two small Rust terminal tools that work like `gzip` / `gunzip`: one direction each, both fully local.

- **`mdspeak`** — renders Markdown nicely in the terminal and reads it aloud with [Piper](https://github.com/rhasspy/piper).
- **`mdhear`** — records your voice (or takes an audio file) and transcribes it to Markdown with [whisper.cpp](https://github.com/ggml-org/whisper.cpp).

Linux only for now. Needs `curl`, `tar`, `aplay` (for `mdspeak`), `pw-record` or `arecord`, `ffmpeg` (for `mdhear -f`), and `git`, `cmake`, a C++ compiler (to build whisper.cpp).

## Install

```bash
cargo install --path .
mdspeak setup   # downloads Piper + an English voice
mdhear setup    # builds whisper.cpp + downloads the base.en model
```

## mdspeak

```bash
mdspeak -f README.md          # render + read a file
mdspeak "# Hi\nSome **bold**" # text from arguments
cat notes.md | mdspeak        # stdin
mdspeak -f notes.md -s 1.3    # faster; -o out.wav saves audio; -q no render; --no-speak render only
```

Markdown is stripped before speaking; code blocks are skipped.

## mdhear

```bash
mdhear                  # record, press Enter to stop, Markdown on stdout
mdhear -o notes.md      # save to a file
mdhear -f meeting.mp3   # transcribe an existing audio file
mdhear --plain          # raw transcript, no voice commands
```

Spoken commands: `new paragraph`, `new line`, `heading one|two|three <sentence>`, `bullet <sentence>`, `numbered item <sentence>`, `bold … end bold`, `italic … end italic`, `code … end code`.

Models/voices live in `~/.local/share/mdspeak` and `~/.local/share/mdhear`.
