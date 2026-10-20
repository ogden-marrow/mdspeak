# mdspeak / mdhear

Two small Rust terminal tools that work like `gzip` / `gunzip`: one direction each, both fully local.

- **`mdspeak`** — renders Markdown nicely in the terminal and reads it aloud with [Piper](https://github.com/rhasspy/piper).
- **`mdhear`** — records your voice (or takes an audio file) and transcribes it to Markdown with [whisper.cpp](https://github.com/ggml-org/whisper.cpp).

Linux only for now. Needs `curl`, `tar`, `aplay` (for `mdspeak`), `pw-record` or `arecord`, `ffmpeg` (for transcribing files), and `git`, `cmake`, a C++ compiler (to build whisper.cpp).

## Install

```bash
make install PREFIX=$HOME/.local   # or plain `sudo make install` for /usr/local
mdspeak setup   # downloads Piper + an English voice
mdhear setup    # builds whisper.cpp + downloads the base.en model
```

`make install` puts the binaries, man pages and bash/zsh/fish completions under `PREFIX` (`DESTDIR` is honored; `make uninstall` removes them). Make sure `$PREFIX/bin` is on your `PATH`.

## mdspeak

```bash
mdspeak README.md             # render + read files
mdspeak -t "# Hi\nSome **bold**"   # text from an option
cat notes.md | mdspeak        # stdin (or `-`)
mdspeak -s 1.3 notes.md       # faster; -o out.wav saves audio; -q no render; -n render only
```

Silent when piped (like any filter); the rendered view shows only on a terminal. Markdown is stripped before speaking; code blocks are skipped.

## mdhear

```bash
mdhear                  # record, press Enter to stop, Markdown on stdout
mdhear -o notes.md      # save to a file
mdhear meeting.mp3      # transcribe audio files (or `-` for stdin)
mdhear --plain          # raw transcript, no voice commands
```

Spoken commands: `new paragraph`, `new line`, `heading one|two|three <sentence>`, `bullet <sentence>`, `numbered item <sentence>`, `bold … end bold`, `italic … end italic`, `code … end code`.

## Unix behavior

Positional files and `-` for stdin; results on stdout, diagnostics on stderr as `prog: message`; exit 0/1/2 (ok/error/usage); no output on success when piped; `NO_COLOR` respected; `SIGPIPE` handled (`| head` works); `--help`, `--version`, `--completions SHELL`; env defaults `MDSPEAK_VOICE`, `MDSPEAK_SPEED`, `MDHEAR_MODEL`; data under `$XDG_DATA_HOME`. See `man mdspeak` and `man mdhear`.
