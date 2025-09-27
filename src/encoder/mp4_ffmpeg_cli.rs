//! MP4 encoder using ffmpeg CLI with real-time streaming (ffmpeg must be in PATH).

use super::{Encoder, Result};
use bevy::prelude::*;
use std::{
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
};

/// An encoder that streams frames directly to ffmpeg for real-time MP4 encoding.
/// ffmpeg must be in PATH.
pub struct Mp4FfmpegCliEncoder {
    /// The ffmpeg child process
    process: Option<Child>,

    /// Output file path
    path: PathBuf,

    /// Video configuration
    framerate: u32,
    crf: u32,
    preset: Option<String>,

    /// Hardware encoder preference (nvenc, vaapi, etc.)
    hardware_encoder: Option<String>,

    /// Video resolution (width, height)
    resolution: Option<(u32, u32)>,
}

impl Mp4FfmpegCliEncoder {
    /// Creates a new MP4 encoder that writes the MP4 to the given path.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self {
            process: None,
            path: path.into(),
            framerate: 60,
            crf: 23,
            preset: Some("fast".to_string()),
            hardware_encoder: None,
            resolution: None,
        })
    }

    /// Sets the framerate of the video.
    pub fn with_framerate(mut self, framerate: u32) -> Self {
        self.framerate = framerate;
        self
    }

    /// Sets the CRF (Constant Rate Factor) of the video.
    pub fn with_crf(mut self, crf: u32) -> Self {
        self.crf = crf;
        self
    }
    
    /// Sets the preset of the video.
    pub fn with_preset(mut self, preset: impl Into<String>) -> Self {
        self.preset = Some(preset.into());
        self
    }

    /// Sets hardware encoder (e.g., "h264_nvenc", "h264_vaapi", "h264_videotoolbox").
    /// If the hardware encoder is not available, it will fallback to libx264.
    pub fn with_hardware_encoder(mut self, encoder: impl Into<String>) -> Self {
        self.hardware_encoder = Some(encoder.into());
        self
    }

    /// Explicitly sets the video resolution. If not set, resolution will be determined from the first frame.
    pub fn with_resolution(mut self, width: u32, height: u32) -> Self {
        self.resolution = Some((width, height));
        self
    }

    /// Detects available hardware encoders on the system
    pub fn detect_hardware_encoder() -> Option<String> {
        // Try NVIDIA NVENC first
        if Self::test_encoder("h264_nvenc") {
            return Some("h264_nvenc".to_string());
        }

        // Try Intel Quick Sync Video
        if Self::test_encoder("h264_qsv") {
            return Some("h264_qsv".to_string());
        }

        // Try VAAPI (Linux)
        if Self::test_encoder("h264_vaapi") {
            return Some("h264_vaapi".to_string());
        }

        // Try VideoToolbox (macOS)
        if Self::test_encoder("h264_videotoolbox") {
            return Some("h264_videotoolbox".to_string());
        }

        None
    }

    /// Tests if a specific encoder is available
    fn test_encoder(encoder: &str) -> bool {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(encoder))
            .unwrap_or(false)
    }

    /// Initializes the ffmpeg process with the first frame to determine video properties
    fn initialize_process(&mut self, image: &Image) -> Result<()> {
        let (width, height) = match self.resolution {
            Some((w, h)) => (w, h),
            None => (image.width(), image.height()),
        };
        
        // Preset
        let preset = self.preset.as_deref().unwrap();

        // Choose encoder
        let encoder = self
            .hardware_encoder
            .clone()
            .or_else(|| Self::detect_hardware_encoder())
            .unwrap_or_else(|| "libx264".to_string());

        bevy::log::info!("Using encoder: {} for {}x{} video", encoder, width, height);

        let mut command = Command::new("ffmpeg");

        // Input settings
        command.args([
            "-y", // Overwrite output file
            "-f",
            "rawvideo", // Input format
            "-pix_fmt",
            "rgba", // Input pixel format (Bevy uses RGBA)
            "-s",
            &format!("{}x{}", width, height), // Input resolution
            "-r",
            &self.framerate.to_string(), // Input framerate
            "-i",
            "pipe:0", // Read from stdin
        ]);

        // Encoder settings
        command.args(["-c:v", &encoder]);

        // Pixel format for output
        command.args(["-pix_fmt", "yuv420p"]);

        // Quality settings - adjust based on encoder type
        match encoder.as_str() {
            "h264_nvenc" => {
                command.args(["-preset", preset]);
                command.args(["-cq", &self.crf.to_string()]);
            }
            "h264_qsv" => {
                command.args(["-preset", preset]);
                command.args(["-global_quality", &self.crf.to_string()]);
            }
            "h264_vaapi" => {
                command.args(["-vaapi_device", "/dev/dri/renderD128"]);
                command.args(["-qp", &self.crf.to_string()]);
            }
            "h264_videotoolbox" => {
                command.args(["-q:v", &self.crf.to_string()]);
            }
            _ => {
                // Default libx264 settings
                command.args(["-preset", preset]);
                command.args(["-crf", &self.crf.to_string()]);
            }
        }

        // Output file
        command.arg(&self.path);

        // Set up pipes
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = command.spawn()?;
        self.process = Some(child);

        Ok(())
    }

    /// Converts Bevy Image (RGBA) to raw bytes suitable for ffmpeg
    fn image_to_raw_bytes(image: &Image) -> Result<Vec<u8>> {
        // Bevy images are typically in RGBA format
        let image_data = image.data.as_ref().ok_or("Image has no data")?;

        // ffmpeg expects raw RGBA bytes
        Ok(image_data.clone())
    }
}

impl Encoder for Mp4FfmpegCliEncoder {
    fn encode(&mut self, image: &Image) -> Result<()> {
        // Initialize process on first frame
        if self.process.is_none() {
            self.initialize_process(image)?;
        }

        // Get the stdin pipe
        let process = self
            .process
            .as_mut()
            .ok_or("FFmpeg process not initialized")?;

        let stdin = process.stdin.as_mut().ok_or("Failed to get stdin pipe")?;

        // Convert image to raw bytes and write to ffmpeg
        let raw_bytes = Self::image_to_raw_bytes(image)?;
        stdin.write_all(&raw_bytes)?;
        stdin.flush()?;

        Ok(())
    }

    fn finish(mut self: Box<Self>) {
        if let Some(mut process) = self.process.take() {
            // Close stdin to signal end of input
            drop(process.stdin.take());

            // Wait for process to complete
            match process.wait() {
                Ok(status) => {
                    if status.success() {
                        bevy::log::info!("Video encoding completed successfully: {:?}", self.path);
                    } else {
                        // Try to capture stderr for error details
                        if let Some(mut stderr) = process.stderr.take() {
                            use std::io::Read;
                            let mut error_msg = String::new();
                            if stderr.read_to_string(&mut error_msg).is_ok() {
                                bevy::log::error!(
                                    "FFmpeg failed with status {}: {}",
                                    status,
                                    error_msg
                                );
                            } else {
                                bevy::log::error!("FFmpeg failed with status: {}", status);
                            }
                        } else {
                            bevy::log::error!("FFmpeg failed with status: {}", status);
                        }
                    }
                }
                Err(error) => {
                    bevy::log::error!("Failed to wait for FFmpeg process: {:?}", error);
                }
            }
        }
    }
}

impl Drop for Mp4FfmpegCliEncoder {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            // Attempt graceful shutdown
            drop(process.stdin.take());

            // Give it a moment to finish
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Force kill if still running
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}
