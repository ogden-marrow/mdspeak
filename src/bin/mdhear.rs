use std::io::{BufRead, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

const WHISPER_REPO: &str = "https://github.com/ggml-org/whisper.cpp";
const MODEL_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
const DEFAULT_MODEL: &str = "base.en";

#[derive(Parser)]
#[command(
    name = "mdhear",
    version,
    about = "Dictate Markdown: record your voice (or read an audio file) and transcribe it locally with whisper.cpp",
    after_help = "Voice commands (disable with --plain):\n  \"new paragraph\" / \"new line\"\n  \"heading one|two|three <sentence>\"\n  \"bullet <sentence>\"  \"numbered item <sentence>\"\n  \"bold ... end bold\"  \"italic ... end italic\"  \"code ... end code\""
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,

    /// Write the Markdown to this file instead of stdout ("-" = stdout)
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Whisper model name (e.g. base.en, small.en) or path to a .bin file
    #[arg(short, long, default_value = DEFAULT_MODEL, env = "MDHEAR_MODEL")]
    model: String,

    /// Don't interpret spoken Markdown commands; output the raw transcript
    #[arg(long)]
    plain: bool,

    /// Suppress status messages on stderr
    #[arg(short, long)]
    quiet: bool,

    /// Print a shell completion script and exit
    #[arg(long, value_name = "SHELL")]
    completions: Option<Shell>,

    /// Audio files to transcribe ("-" = audio on stdin); with none, record from the microphone
    #[arg(value_name = "FILE")]
    files: Vec<PathBuf>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build whisper.cpp and download a model into ~/.local/share/mdhear
    Setup {
        /// Model to download (tiny.en, base.en, small.en, medium.en, ...)
        #[arg(default_value = DEFAULT_MODEL)]
        model: String,
    },
}

fn data_dir() -> Result<PathBuf> {
    Ok(dirs::data_local_dir().context("no data dir")?.join("mdhear"))
}

fn cli_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("whisper.cpp/build/bin/whisper-cli"))
}

fn run(cmd: &mut Command) -> Result<()> {
    let st = cmd.status().with_context(|| format!("failed to run {:?}", cmd.get_program()))?;
    if !st.success() {
        bail!("{:?} exited with {}", cmd.get_program(), st);
    }
    Ok(())
}

fn setup(model: &str) -> Result<()> {
    let dir = data_dir()?;
    std::fs::create_dir_all(dir.join("models"))?;
    let src = dir.join("whisper.cpp");
    if !src.exists() {
        println!("Cloning whisper.cpp...");
        run(Command::new("git").args(["clone", "--depth", "1", WHISPER_REPO]).arg(&src))?;
    }
    if !cli_path()?.exists() {
        println!("Building whisper.cpp (a few minutes)...");
        run(Command::new("cmake").current_dir(&src).args(["-B", "build", "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=OFF"]))?;
        run(Command::new("cmake").current_dir(&src).args(["--build", "build", "-j", "--config", "Release", "--target", "whisper-cli"]))?;
    }
    let dest = dir.join(format!("models/ggml-{model}.bin"));
    if !dest.exists() {
        println!("Downloading model {model}...");
        run(Command::new("curl").args(["-L", "--fail", "-o"]).arg(&dest).arg(format!("{MODEL_BASE}/ggml-{model}.bin")))?;
    }
    println!("Done. Try: mdhear   (press Enter to stop recording)");
    Ok(())
}

fn model_path(model: &str) -> Result<PathBuf> {
    let p = PathBuf::from(model);
    let path = if p.extension().is_some_and(|e| e == "bin") { p } else { data_dir()?.join(format!("models/ggml-{model}.bin")) };
    if !path.exists() {
        bail!("model not found at {}. Run `mdhear setup` first.", path.display());
    }
    Ok(path)
}

/// Record the mic to a 16 kHz mono WAV until the user presses Enter.
fn record(wav: &Path, quiet: bool) -> Result<()> {
    let mut cmd = if which("pw-record") {
        let mut c = Command::new("pw-record");
        c.args(["--rate", "16000", "--channels", "1", "--format", "s16"]).arg(wav);
        c
    } else if which("arecord") {
        let mut c = Command::new("arecord");
        c.args(["-q", "-f", "S16_LE", "-r", "16000", "-c", "1", "-t", "wav"]).arg(wav);
        c
    } else {
        bail!("need pw-record or arecord to record audio");
    };
    let mut child = cmd.stderr(Stdio::null()).spawn().context("failed to start recorder")?;
    if !quiet {
        eprintln!("mdhear: recording... press Enter to stop");
    }
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    // SIGINT lets the recorder finalize the WAV header.
    Command::new("kill").args(["-INT", &child.id().to_string()]).status()?;
    child.wait()?;
    Ok(())
}

fn which(bin: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {bin}")]).stdout(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Convert any audio file to 16 kHz mono WAV for whisper.
fn convert(input: &Path, wav: &Path) -> Result<()> {
    let src = if input.as_os_str() == "-" { PathBuf::from("pipe:0") } else { input.to_path_buf() };
    run(Command::new("ffmpeg").args(["-y", "-loglevel", "error", "-i"]).arg(src).args(["-ar", "16000", "-ac", "1"]).arg(wav))
}

fn transcribe(wav: &Path, model: &Path) -> Result<String> {
    let bin = cli_path()?;
    if !bin.exists() {
        bail!("whisper.cpp not built. Run `mdhear setup` first.");
    }
    let out = Command::new(bin)
        .arg("-m").arg(model).arg("-f").arg(wav)
        .args(["-nt", "-np", "-l", "en"])
        .stderr(Stdio::null())
        .output()
        .context("failed to run whisper-cli")?;
    if !out.status.success() {
        bail!("whisper-cli failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).split_whitespace().collect::<Vec<_>>().join(" "))
}

/// True when the output currently ends in an opening inline mark (no space wanted after it).
fn open_mark(out: &str) -> bool {
    for m in ["**", "*", "`"] {
        if let Some(rest) = out.strip_suffix(m) {
            // an opening mark follows whitespace/line start; a closing mark follows text
            return rest.is_empty() || rest.ends_with([' ', '\n']);
        }
    }
    false
}

fn norm(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

fn ends_sentence(w: &str) -> bool {
    w.trim_end_matches(['"', '\'', ')']).ends_with(['.', '?', '!'])
}

/// Turn a transcript containing spoken commands into Markdown.
fn to_markdown(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = String::new();
    let mut at_line_start = true;
    let mut prefix: Option<String> = None; // pending line prefix (heading/bullet)
    let mut block_line = false; // inside a heading/bullet line: ends at sentence end
    let mut i = 0;

    let push_word = |out: &mut String, w: &str, at_start: &mut bool, prefix: &mut Option<String>| {
        if let Some(p) = prefix.take() {
            out.push_str(&p);
        } else if !*at_start && !open_mark(out) {
            out.push(' ');
        }
        out.push_str(w);
        *at_start = false;
    };

    while i < words.len() {
        let w = norm(words[i]);
        let next = words.get(i + 1).map(|s| norm(s)).unwrap_or_default();
        let mut consumed = 0;

        match (w.as_str(), next.as_str()) {
            ("new", "paragraph") => {
                out.truncate(out.trim_end().len());
                out.push_str("\n\n");
                at_line_start = true;
                block_line = false;
                consumed = 2;
            }
            ("new", "line") | ("newline", _) => {
                out.push('\n');
                at_line_start = true;
                block_line = false;
                consumed = if w == "new" { 2 } else { 1 };
            }
            ("heading" | "header", "one" | "1" | "two" | "2" | "three" | "3") => {
                let level = match next.as_str() { "one" | "1" => 1, "two" | "2" => 2, _ => 3 };
                if !out.is_empty() && !out.ends_with("\n\n") {
                    out.push_str(if out.ends_with('\n') { "\n" } else { "\n\n" });
                }
                prefix = Some(format!("{} ", "#".repeat(level)));
                at_line_start = true;
                block_line = true;
                consumed = 2;
            }
            ("bullet", n) => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                prefix = Some("- ".into());
                at_line_start = true;
                block_line = true;
                consumed = if n == "point" { 2 } else { 1 };
            }
            ("numbered" | "number", "item") => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                prefix = Some("1. ".into());
                at_line_start = true;
                block_line = true;
                consumed = 2;
            }
            ("bold", _) | ("italic", _) | ("code", _) => {
                let mark = match w.as_str() { "bold" => "**", "italic" => "*", _ => "`" };
                if let Some(p) = prefix.take() { out.push_str(&p); } else if !at_line_start { out.push(' '); }
                out.push_str(mark);
                at_line_start = false;
                consumed = 1;
            }
            ("end", "bold" | "italic" | "code") => {
                let mark = match next.as_str() { "bold" => "**", "italic" => "*", _ => "`" };
                out.push_str(mark);
                consumed = 2;
            }
            _ => {}
        }

        if consumed > 0 {
            i += consumed;
            // keep trailing punctuation that was attached to the command words
            let last = words[i - 1];
            if block_line && prefix.is_none() && ends_sentence(last) {
                out.push('\n');
                at_line_start = true;
                block_line = false;
            }
            continue;
        }

        let word = words[i];
        if block_line && ends_sentence(word) {
            let trimmed = word.trim_end_matches(['.', '!']);
            push_word(&mut out, trimmed, &mut at_line_start, &mut prefix);
            out.push('\n');
            at_line_start = true;
            block_line = false;
        } else {
            push_word(&mut out, word, &mut at_line_start, &mut prefix);
        }
        i += 1;
    }
    let mut s = out.trim().to_string();
    s.push('\n');
    s
}

fn main() {
    // Die quietly on a closed pipe (`mdhear | head`) like other Unix tools.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    if let Err(e) = real_main() {
        eprintln!("mdhear: {e:#}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(shell) = cli.completions {
        clap_complete::generate(shell, &mut Cli::command(), "mdhear", &mut std::io::stdout());
        return Ok(());
    }
    if let Some(Cmd::Setup { model }) = &cli.cmd {
        return setup(model);
    }
    let quiet = cli.quiet || !std::io::stderr().is_terminal();
    let model = model_path(&cli.model)?;
    let tmp = std::env::temp_dir().join(format!("mdhear-{}", std::process::id()));
    std::fs::create_dir_all(&tmp)?;

    let result = (|| -> Result<Vec<String>> {
        let mut texts = Vec::new();
        if cli.files.is_empty() {
            let wav = tmp.join("audio.wav");
            record(&wav, quiet)?;
            texts.push(transcribe_logged(&wav, &model, quiet)?);
        }
        for (i, f) in cli.files.iter().enumerate() {
            let wav = tmp.join(format!("audio{i}.wav"));
            convert(f, &wav).with_context(|| f.display().to_string())?;
            texts.push(transcribe_logged(&wav, &model, quiet)?);
        }
        Ok(texts)
    })();
    std::fs::remove_dir_all(&tmp).ok();
    let texts = result?;
    if texts.iter().all(|t| t.trim().is_empty()) {
        bail!("no speech detected");
    }

    let render = |t: &str| if cli.plain { format!("{t}\n") } else { to_markdown(t) };
    let md = texts.iter().filter(|t| !t.trim().is_empty()).map(|t| render(t)).collect::<Vec<_>>().join("\n");
    match &cli.output {
        Some(p) if p.as_os_str() != "-" => std::fs::write(p, &md).with_context(|| p.display().to_string())?,
        _ => print!("{md}"),
    }
    Ok(())
}

fn transcribe_logged(wav: &Path, model: &Path, quiet: bool) -> Result<String> {
    if !quiet {
        eprintln!("mdhear: transcribing...");
    }
    transcribe(wav, model)
}

#[cfg(test)]
mod tests {
    use super::to_markdown;

    #[test]
    fn commands() {
        let t = "Heading one. Shopping list. New paragraph. Things I need: Bullet milk. Bullet bold eggs end bold. New paragraph. Some code x end code is fine.";
        assert_eq!(
            to_markdown(t),
            "# Shopping list\n\nThings I need:\n- milk\n- **eggs**\n\nSome `x` is fine.\n"
        );
    }
}
