use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use pulldown_cmark::{Event, Options, Parser as MdParser, Tag, TagEnd};

const PIPER_URL: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz";
const VOICE_BASE: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium";
const VOICE_NAME: &str = "en_US-lessac-medium";

#[derive(Parser)]
#[command(name = "mdspeak", version, about = "Render Markdown nicely in the terminal and read it aloud with a local TTS model (Piper)")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,

    /// Markdown file to read. Omit to use the text arguments or stdin.
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Voice model name (in the voices dir) or path to a .onnx file
    #[arg(short, long)]
    voice: Option<String>,

    /// Save audio to a WAV file instead of playing it
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Print the rendered Markdown only, don't speak
    #[arg(long)]
    no_speak: bool,

    /// Speak only, don't print the rendered Markdown
    #[arg(short, long)]
    quiet: bool,

    /// Speed multiplier (1.0 = normal, 1.5 = faster)
    #[arg(short, long, default_value_t = 1.0)]
    speed: f32,

    /// Text to speak (Markdown allowed)
    text: Vec<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Download Piper and a default English voice into ~/.local/share/mdspeak
    Setup,
}

fn data_dir() -> Result<PathBuf> {
    Ok(dirs::data_local_dir().context("no data dir")?.join("mdspeak"))
}

fn run(cmd: &mut Command) -> Result<()> {
    let st = cmd.status().with_context(|| format!("failed to run {:?}", cmd.get_program()))?;
    if !st.success() {
        bail!("{:?} exited with {}", cmd.get_program(), st);
    }
    Ok(())
}

fn setup() -> Result<()> {
    let dir = data_dir()?;
    let voices = dir.join("voices");
    std::fs::create_dir_all(&voices)?;
    if !dir.join("piper/piper").exists() {
        println!("Downloading Piper...");
        let tgz = dir.join("piper.tar.gz");
        run(Command::new("curl").args(["-L", "--fail", "-o"]).arg(&tgz).arg(PIPER_URL))?;
        run(Command::new("tar").arg("-xzf").arg(&tgz).arg("-C").arg(&dir))?;
        std::fs::remove_file(&tgz).ok();
    }
    for ext in ["onnx", "onnx.json"] {
        let dest = voices.join(format!("{VOICE_NAME}.{ext}"));
        if !dest.exists() {
            println!("Downloading voice {VOICE_NAME}.{ext}...");
            run(Command::new("curl").args(["-L", "--fail", "-o"]).arg(&dest).arg(format!("{VOICE_BASE}/{VOICE_NAME}.{ext}")))?;
        }
    }
    println!("Done. Try: mdspeak \"# Hello\\nThis is **local** speech.\"");
    Ok(())
}

/// Turn Markdown into plain, speakable text (one utterance per line).
fn markdown_to_speech(md: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for ev in MdParser::new_ext(md, Options::all()) {
        match ev {
            Event::Start(Tag::CodeBlock(_)) => {
                in_code = true;
                out.push_str("Code block omitted.\n");
            }
            Event::End(TagEnd::CodeBlock) => in_code = false,
            Event::Text(t) if !in_code => out.push_str(&t),
            Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            Event::End(TagEnd::Heading(_)) => out.push_str(".\n"),
            Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::TableRow | TagEnd::TableHead) => out.push('\n'),
            Event::End(TagEnd::TableCell) => out.push_str(", "),
            Event::Start(Tag::Image { .. }) => out.push_str("Image: "),
            _ => {}
        }
    }
    out.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

fn resolve_voice(voice: Option<&str>) -> Result<PathBuf> {
    let voices = data_dir()?.join("voices");
    let v = voice.unwrap_or(VOICE_NAME);
    let p = PathBuf::from(v);
    let path = if p.extension().is_some_and(|e| e == "onnx") { p } else { voices.join(format!("{v}.onnx")) };
    if !path.exists() {
        bail!("voice not found at {}. Run `mdspeak setup` first.", path.display());
    }
    Ok(path)
}

fn speak(text: &str, voice: Option<&str>, output: Option<&PathBuf>, speed: f32) -> Result<()> {
    let piper = data_dir()?.join("piper/piper");
    if !piper.exists() {
        bail!("Piper not installed. Run `mdspeak setup` first.");
    }
    let model = resolve_voice(voice)?;
    let mut cmd = Command::new(&piper);
    cmd.arg("--model").arg(&model).arg("--length_scale").arg((1.0 / speed.max(0.1)).to_string());
    cmd.stderr(Stdio::null()).stdin(Stdio::piped());
    match output {
        Some(path) => {
            cmd.arg("--output_file").arg(path);
        }
        None => {
            cmd.arg("--output_raw").stdout(Stdio::piped());
        }
    }
    let mut piper_child = cmd.spawn().context("failed to start piper")?;
    let mut player = if output.is_none() {
        let rate = sample_rate(&model).unwrap_or(22050).to_string();
        let raw = piper_child.stdout.take().unwrap();
        Some(
            Command::new("aplay")
                .args(["-q", "-r", &rate, "-f", "S16_LE", "-t", "raw", "-"])
                .stdin(raw)
                .spawn()
                .context("failed to start aplay")?,
        )
    } else {
        None
    };
    piper_child.stdin.take().unwrap().write_all(text.as_bytes())?;
    piper_child.wait()?;
    if let Some(p) = player.as_mut() {
        p.wait()?;
    }
    if let Some(path) = output {
        println!("Saved {}", path.display());
    }
    Ok(())
}

fn sample_rate(model: &PathBuf) -> Option<u32> {
    let json = std::fs::read_to_string(format!("{}.json", model.display())).ok()?;
    let i = json.find("\"sample_rate\"")?;
    json[i..].split(|c: char| !c.is_ascii_digit()).find(|s| !s.is_empty())?.parse().ok()
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Cmd::Setup) = cli.cmd {
        return setup();
    }

    let md = if let Some(f) = &cli.file {
        std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?
    } else if !cli.text.is_empty() {
        cli.text.join(" ").replace("\\n", "\n")
    } else {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    };
    if md.trim().is_empty() {
        bail!("nothing to read");
    }

    if !cli.quiet {
        termimad::print_text(&md);
    }
    if !cli.no_speak {
        speak(&markdown_to_speech(&md), cli.voice.as_deref(), cli.output.as_ref(), cli.speed)?;
    }
    Ok(())
}
