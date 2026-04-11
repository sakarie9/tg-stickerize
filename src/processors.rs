use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};
use image::imageops::FilterType;
use image::{GenericImageView, ImageReader};

pub async fn process_image(input_path: &Path, output_path: &Path) -> Result<()> {
    // 加载图片
    let img = ImageReader::open(input_path)?
        .with_guessed_format()?
        .decode()?;

    // 获取原始尺寸
    let (width, height) = img.dimensions();

    // 计算新尺寸，确保至少一边是512像素
    let (new_width, new_height) = if width >= height {
        let ratio = 512.0 / width as f32;
        (512, (height as f32 * ratio).round() as u32)
    } else {
        let ratio = 512.0 / height as f32;
        ((width as f32 * ratio).round() as u32, 512)
    };

    // 调整尺寸
    let resized = img.resize_exact(new_width, new_height, FilterType::Lanczos3);

    // 保存为WebP格式，质量80%（可根据需要调整）
    resized.save_with_format(output_path, image::ImageFormat::WebP)?;

    // 检查文件大小
    let file_size = fs::metadata(output_path)?.len();
    if file_size > 512 * 1024 {
        // 如果文件太大，尝试进一步压缩
        return Err(anyhow!(
            "图片太大 ({}KB)，即使压缩后仍超过512KB限制",
            file_size / 1024
        ));
    }

    Ok(())
}

pub async fn process_webm(input_path: &Path, output_path: &Path) -> Result<()> {
    // 使用ffprobe获取视频信息，改用JSON格式
    let mut command = Command::new("ffprobe");
    let output = command.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height,r_frame_rate:format=duration",
        "-of",
        "json",
        input_path.to_str().unwrap(),
    ]);
    log::debug!("FFprobe command: {:?}", &output);
    let output = output.output()?;
    if !output.status.success() {
        return Err(anyhow!("FFprobe命令执行失败"));
    }

    // 解析JSON响应
    let json_str = String::from_utf8(output.stdout)?;
    let json: serde_json::Value = serde_json::from_str(&json_str)?;

    // 从JSON中提取视频信息
    let streams = json["streams"]
        .as_array()
        .ok_or_else(|| anyhow!("无法获取视频流信息"))?;
    if streams.is_empty() {
        return Err(anyhow!("无法找到视频流"));
    }
    let stream = &streams[0];
    let width = stream["width"]
        .as_u64()
        .ok_or_else(|| anyhow!("无法获取视频宽度"))? as u32;
    let height = stream["height"]
        .as_u64()
        .ok_or_else(|| anyhow!("无法获取视频高度"))? as u32;
    let format = json["format"]
        .as_object()
        .ok_or_else(|| anyhow!("无法获取视频格式信息"))?;
    let duration_str = format["duration"]
        .as_str()
        .ok_or_else(|| anyhow!("无法获取视频时长"))?;
    let duration = duration_str.parse::<f32>()?;

    // 处理帧率
    let fps_str = stream["r_frame_rate"]
        .as_str()
        .ok_or_else(|| anyhow!("无法获取视频帧率"))?;
    let fps_parts: Vec<&str> = fps_str.split('/').collect();
    let fps = if fps_parts.len() == 2 {
        let num: f32 = fps_parts[0].parse().context("无法解析帧率分子")?;
        let den: f32 = fps_parts[1].parse().context("无法解析帧率分母")?;
        if den == 0.0 {
            return Err(anyhow!("帧率分母为零"));
        }
        num / den
    } else {
        fps_str.parse().context("无法解析帧率")?
    };

    // 计算新尺寸，确保至少一边是512像素
    let (new_width, new_height) = if width >= height {
        let ratio = 512.0 / width as f32;
        (512, (height as f32 * ratio).round() as u32)
    } else {
        let ratio = 512.0 / height as f32;
        ((width as f32 * ratio).round() as u32, 512)
    };

    // 设置帧率限制和时长限制
    let target_fps = if fps > 30.0 { 30 } else { fps.round() as u32 };
    let target_duration = if duration > 3.0 { 3.0 } else { duration };

    // 使用FFmpeg处理视频
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            input_path.to_str().unwrap(),
            "-t",
            &target_duration.to_string(),
            "-vf",
            &format!("scale={}:{}", new_width, new_height),
            "-r",
            &target_fps.to_string(),
            "-c:v",
            "libvpx-vp9",
            "-b:v",
            "200k",
            "-auto-alt-ref",
            "0",
            "-pix_fmt",
            "yuva420p",
            "-f",
            "webm",
            output_path.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !status.success() {
        return Err(anyhow!("FFmpeg命令执行失败"));
    }

    // 检查文件大小
    let file_size = fs::metadata(output_path)?.len();
    if file_size > 256 * 1024 {
        return Err(anyhow!(
            "视频太大 ({}KB)，即使压缩后仍超过256KB限制",
            file_size / 1024
        ));
    }

    Ok(())
}

pub async fn process_video_to_gif(input_path: &Path, output_path: &Path) -> Result<()> {
    // 使用 FFmpeg 生成 GIF，保留原始分辨率和帧率
    // 使用 split[s0][s1];[s0]palettegen=[s1]paletteuse 流水线生成优化调色板
    let filter_complex = "split[s0][s1];[s0]palettegen=max_colors=128[p];[s1][p]paletteuse";

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            input_path.to_str().unwrap(),
            "-vf",
            filter_complex,
            "-f",
            "gif",
            output_path.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !status.success() {
        return Err(anyhow!("FFmpeg GIF生成失败"));
    }

    // 检查文件大小（Telegram Bot API 限制 20MB）
    let file_size = fs::metadata(output_path)?.len();
    if file_size > 20 * 1024 * 1024 {
        return Err(anyhow!(
            "GIF文件太大 ({}MB)，超过20MB限制",
            file_size / (1024 * 1024)
        ));
    }

    Ok(())
}
