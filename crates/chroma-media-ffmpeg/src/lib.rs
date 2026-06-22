//! Chroma FFmpeg-backed encoder.
//!
//! Implements [`chroma_media_api::Encoder`] by piping composited RGBA8 frames
//! to the `ffmpeg` command-line tool over stdin:
//!
//! - **MP4** via `libx264`/`libx265`/`libvpx-vp9`, CRF or target-bitrate rate
//!   control, `yuv420p` for broad playback (spec EXP-01/04).
//! - **GIF** via a single ffmpeg invocation using the two-pass `palettegen` +
//!   `paletteuse` filter chain — the per-clip optimized palette the spec calls
//!   for on gradient backgrounds (spec EXP-02/05).
//!
//! The subprocess approach (spec §1.1: "FFmpeg … via bindings, or subprocess")
//! keeps the dependency surface to the system `ffmpeg` binary — no native
//! linking, no Rust deps. Hardware encode (VAAPI/NVENC, spec EXP-08) is a later
//! addition to [`build_args`]; M3 is software.

use chroma_core_api::TimeStamp;
use chroma_media_api::{
    Codec, Container, Encoder, MediaError, OutputSpec, RateControl, Result, RgbaFrame,
};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread::JoinHandle;

/// Whether an `ffmpeg` binary is callable on the current `PATH`.
///
/// Useful for examples and tests to skip cleanly when ffmpeg is absent.
pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// An [`Encoder`] that streams frames to an `ffmpeg` subprocess.
///
/// Lifecycle is `open` → `push_frame`* → `finish`. Frames must be pushed in
/// order at the [`OutputSpec::fps`] rate and match [`OutputSpec::canvas`]; the
/// `pts` argument is advisory (rawvideo input is constant-rate).
pub struct FfmpegEncoder {
    out_path: PathBuf,
    canvas_len: usize,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stderr: Option<JoinHandle<Vec<u8>>>,
}

impl FfmpegEncoder {
    /// Creates an encoder that will write to `out_path` once [`open`](Encoder::open)
    /// is called.
    pub fn new(out_path: impl Into<PathBuf>) -> Self {
        FfmpegEncoder {
            out_path: out_path.into(),
            canvas_len: 0,
            child: None,
            stdin: None,
            stderr: None,
        }
    }
}

impl Encoder for FfmpegEncoder {
    fn open(&mut self, spec: &OutputSpec) -> Result<()> {
        let args = build_args(spec, &self.out_path);
        let mut child = Command::new("ffmpeg")
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => {
                    MediaError::Unsupported("ffmpeg not found on PATH".to_owned())
                }
                _ => MediaError::Io(e.to_string()),
            })?;

        self.stdin = child.stdin.take();
        // Drain stderr on a thread so ffmpeg never blocks writing logs while we
        // write frames to its stdin (which would deadlock).
        if let Some(mut err) = child.stderr.take() {
            self.stderr = Some(std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = err.read_to_end(&mut buf);
                buf
            }));
        }
        self.child = Some(child);
        self.canvas_len = (spec.canvas.width as usize) * (spec.canvas.height as usize) * 4;
        Ok(())
    }

    fn push_frame(&mut self, frame: &RgbaFrame, _pts: TimeStamp) -> Result<()> {
        if frame.data.len() != self.canvas_len {
            return Err(MediaError::InvalidSpec(format!(
                "frame is {} bytes but the canvas expects {}",
                frame.data.len(),
                self.canvas_len
            )));
        }
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            MediaError::Backend("push_frame called before open/after finish".into())
        })?;
        stdin
            .write_all(&frame.data)
            .map_err(|e| MediaError::Io(e.to_string()))
    }

    fn finish(&mut self) -> Result<()> {
        // Closing stdin signals end-of-stream to ffmpeg.
        self.stdin.take();
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        let status = child.wait().map_err(|e| MediaError::Io(e.to_string()))?;
        let logs = self
            .stderr
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        if status.success() {
            Ok(())
        } else {
            let text = String::from_utf8_lossy(&logs);
            let tail: Vec<&str> = text.lines().rev().take(6).collect();
            let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
            Err(MediaError::Backend(format!(
                "ffmpeg exited with {status}:\n{tail}"
            )))
        }
    }
}

/// Builds the `ffmpeg` argument vector for `spec`, writing to `out`.
///
/// Pure and side-effect free so the command line can be unit-tested without
/// spawning ffmpeg. Input is always rawvideo RGBA on stdin at the spec's size
/// and fps.
pub fn build_args(spec: &OutputSpec, out: &Path) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-y".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgba".into(),
        "-s".into(),
        format!("{}x{}", spec.canvas.width, spec.canvas.height),
        "-r".into(),
        spec.fps.to_string(),
        "-i".into(),
        "-".into(),
    ];

    match spec.container {
        Container::Mp4 if spec.hardware => {
            // VAAPI hardware encode (spec EXP-08): initialize the render node,
            // upload each frame to the GPU, then use the fixed-function encoder.
            // VAAPI uses `-qp` for constant quality (there is no `-crf`).
            a.push("-vaapi_device".into());
            a.push("/dev/dri/renderD128".into());
            a.push("-vf".into());
            a.push("format=nv12,hwupload".into());
            let codec = match spec.codec {
                Codec::H265 => "hevc_vaapi",
                // VP9 VAAPI support is rare; fall back to H.264 VAAPI.
                _ => "h264_vaapi",
            };
            a.push("-c:v".into());
            a.push(codec.into());
            match spec.rate_control {
                RateControl::Crf { crf } => {
                    a.push("-qp".into());
                    a.push(crf.to_string());
                }
                RateControl::Bitrate { bitrate_kbps } => {
                    a.push("-b:v".into());
                    a.push(format!("{bitrate_kbps}k"));
                }
            }
            a.push("-movflags".into());
            a.push("+faststart".into());
        }
        Container::Mp4 => {
            let codec = match spec.codec {
                Codec::H264 => "libx264",
                Codec::H265 => "libx265",
                Codec::Vp9 => "libvpx-vp9",
            };
            a.push("-c:v".into());
            a.push(codec.into());
            match spec.rate_control {
                RateControl::Crf { crf } => {
                    a.push("-crf".into());
                    a.push(crf.to_string());
                }
                RateControl::Bitrate { bitrate_kbps } => {
                    a.push("-b:v".into());
                    a.push(format!("{bitrate_kbps}k"));
                }
            }
            a.push("-pix_fmt".into());
            a.push("yuv420p".into());
            a.push("-movflags".into());
            a.push("+faststart".into());
        }
        Container::Gif => {
            let colors = spec.gif.palette_size.clamp(2, 256);
            let dither = if spec.gif.dithering {
                "bayer:bayer_scale=5"
            } else {
                "none"
            };
            let vf = if spec.gif.two_pass_palette {
                format!(
                    "fps={},split[s0][s1];[s0]palettegen=max_colors={colors}[p];[s1][p]paletteuse=dither={dither}",
                    spec.fps
                )
            } else {
                format!("fps={}", spec.fps)
            };
            a.push("-vf".into());
            a.push(vf);
            // 0 = loop forever (matches GifSettings::loop_count semantics).
            a.push("-loop".into());
            a.push(spec.gif.loop_count.to_string());
        }
    }

    a.push(out.to_string_lossy().into_owned());
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_core_api::Size;
    use chroma_media_api::GifSettings;

    fn mp4_spec() -> OutputSpec {
        OutputSpec {
            container: Container::Mp4,
            canvas: Size::new(640, 360),
            fps: 30,
            codec: Codec::H264,
            rate_control: RateControl::Crf { crf: 20 },
            hardware: false,
            gif: GifSettings {
                palette_size: 256,
                dithering: true,
                two_pass_palette: true,
                loop_count: 0,
            },
        }
    }

    #[test]
    fn mp4_args_select_x264_and_crf() {
        let args = build_args(&mp4_spec(), Path::new("out.mp4"));
        assert!(args.windows(2).any(|w| w == ["-c:v", "libx264"]));
        assert!(args.windows(2).any(|w| w == ["-crf", "20"]));
        assert!(args.windows(2).any(|w| w == ["-s", "640x360"]));
        assert_eq!(args.last().unwrap(), "out.mp4");
    }

    #[test]
    fn bitrate_rate_control_emits_bv() {
        let mut spec = mp4_spec();
        spec.rate_control = RateControl::Bitrate { bitrate_kbps: 4000 };
        let args = build_args(&spec, Path::new("out.mp4"));
        assert!(args.windows(2).any(|w| w == ["-b:v", "4000k"]));
        assert!(!args.iter().any(|s| s == "-crf"));
    }

    #[test]
    fn hardware_mp4_args_select_vaapi() {
        let mut spec = mp4_spec();
        spec.hardware = true;
        let args = build_args(&spec, Path::new("out.mp4"));
        assert!(args.windows(2).any(|w| w == ["-c:v", "h264_vaapi"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["-vf", "format=nv12,hwupload"]));
        // VAAPI uses -qp, never -crf or the software pix_fmt.
        assert!(args.windows(2).any(|w| w == ["-qp", "20"]));
        assert!(!args.iter().any(|s| s == "-crf"));
        assert!(!args.iter().any(|s| s == "libx264"));
    }

    #[test]
    fn hardware_h265_uses_hevc_vaapi() {
        let mut spec = mp4_spec();
        spec.hardware = true;
        spec.codec = Codec::H265;
        let args = build_args(&spec, Path::new("out.mp4"));
        assert!(args.windows(2).any(|w| w == ["-c:v", "hevc_vaapi"]));
    }

    #[test]
    fn gif_args_use_two_pass_palette() {
        let mut spec = mp4_spec();
        spec.container = Container::Gif;
        let args = build_args(&spec, Path::new("out.gif"));
        let vf = args
            .iter()
            .position(|s| s == "-vf")
            .map(|i| &args[i + 1])
            .expect("-vf present");
        assert!(vf.contains("palettegen=max_colors=256"));
        assert!(vf.contains("paletteuse"));
    }
}
