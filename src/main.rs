use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};
use dotenv::dotenv;
use image::GenericImageView;
use image::imageops::FilterType;
use serde_json;
use teloxide::types::InputFile;
use teloxide::{prelude::*, utils::command::BotCommands};
use tempfile::tempdir;
use tokio::fs as tokio_fs;

// 添加命令处理结构
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "支持的命令：")]
enum BotCommand {
    #[command(description = "显示此帮助信息")]
    Help,
    #[command(description = "开始使用bot")]
    Start,
}

async fn process_image(input_path: &Path, output_path: &Path) -> Result<()> {
    // 加载图片
    let img = image::open(input_path)?;

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

async fn process_webm(input_path: &Path, output_path: &Path) -> Result<()> {
    // 使用FFmpeg获取视频信息，改用JSON格式
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate:format=duration",
            "-of",
            "json", // 改用JSON格式输出
            input_path.to_str().unwrap(),
        ])
        .output()?;

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
        let num: f32 = fps_parts[0].parse()?;
        let den: f32 = fps_parts[1].parse()?;
        num / den
    } else {
        fps_str.parse()?
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

async fn handle_file(bot: Bot, msg: Message) -> ResponseResult<()> {
    // 获取文件
    let file = if let Some(photo) = msg.photo() {
        // 获取最高质量的照片
        let photo = photo.last().unwrap();
        bot.get_file(&photo.file.id).await?
    } else if let Some(document) = msg.document() {
        // 检查文件MIME类型
        let mime = document.mime_type.clone().unwrap();
        let mime = mime.subtype().as_str();

        if mime.starts_with("image/")
            || mime.starts_with("video/")
            || mime == "application/octet-stream"
        {
            bot.get_file(&document.file.id).await?
        } else {
            bot.send_message(
                msg.chat.id,
                format!("请发送图片或WebM视频，当前MIME类型: {}", mime),
            )
            .await?;
            return Ok(());
        }
    } else if let Some(sticker) = msg.sticker() {
        // 检查贴纸的MIME类型
        bot.get_file(&sticker.file.id).await?
    } else {
        bot.send_message(msg.chat.id, "请发送图片或WebM视频")
            .await?;
        return Ok(());
    };

    // 创建临时目录
    let temp_dir = tempdir()?;
    let input_path = temp_dir.path().join("input");
    let mut output_path = temp_dir.path().join("output");

    // 下载文件
    let file_url = format!(
        "https://api.telegram.org/file/bot{}/{}",
        bot.token(),
        file.path
    );
    let bytes = reqwest::get(&file_url).await?.bytes().await?;
    tokio_fs::write(&input_path, &bytes).await?;

    // 检测文件类型并处理
    let is_image = infer::get_from_path(&input_path)
        .map(|info| {
            info.map(|i| i.mime_type().starts_with("image/"))
                .unwrap_or(false)
        })
        .unwrap_or(false);

    let is_video = infer::get_from_path(&input_path)
        .map(|info| {
            info.map(|i| i.mime_type().starts_with("video/"))
                .unwrap_or(false)
        })
        .unwrap_or(false);

    let result = if is_image {
        // bot.send_message(msg.chat.id, "正在处理图片...").await?;
        output_path.set_extension("webp");
        process_image(&input_path, &output_path).await
    } else if is_video {
        // bot.send_message(msg.chat.id, "正在处理视频...").await?;
        output_path.set_extension("webm");
        process_webm(&input_path, &output_path).await
    } else {
        Err(anyhow!("不支持的文件类型，请发送图片或WebM视频"))
    };

    // 处理结果
    match result {
        Ok(_) => {
            // 发送处理后的文件
            let input_doc = InputFile::file(&output_path);
            if is_image {
                bot.send_sticker(msg.chat.id, input_doc).await?;
                bot.send_message(
                    msg.chat.id,
                    "这是处理后的贴纸，您可以添加到 @Stickers bot 创建的贴纸包中",
                )
                .await?;
            } else {
                bot.send_sticker(msg.chat.id, input_doc).await?;
                bot.send_message(
                    msg.chat.id,
                    "这是处理后的贴纸，您可以添加到 @Stickers bot 创建的贴纸包中",
                )
                .await?;
            }
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("处理失败: {}", e))
                .await?;
        }
    }

    Ok(())
}

async fn command_handler(bot: Bot, msg: Message, cmd: BotCommand) -> ResponseResult<()> {
    match cmd {
        BotCommand::Help | BotCommand::Start => {
            bot.send_message(
                msg.chat.id,
                "欢迎使用 Telegram Sticker 优化工具！\n\n\
                 请发送图片或WebM视频，我将处理成符合 Telegram 贴纸要求的格式。\n\n\
                 - 图片将被调整至合适尺寸并转换为WebP格式\n\
                 - 视频将被调整至合适尺寸、帧率和时长，转换为WebM格式\n\n\
                 处理后，您可以将文件添加到 @Stickers bot 创建的贴纸包中。",
            )
            .await?;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    dotenv().ok();

    // 初始化日志
    pretty_env_logger::init();
    log::info!("Starting Telegram sticker bot...");

    // 从环境变量获取机器人TOKEN
    let token = std::env::var("TELEGRAM_BOT_TOKEN")
        .context("未找到TELEGRAM_BOT_TOKEN环境变量。请在.env文件中设置或直接设置环境变量")?;

    let bot = Bot::new(token);

    // 创建处理器
    let handler = Update::filter_message()
        .branch(
            dptree::entry()
                .filter_command::<BotCommand>()
                .endpoint(command_handler),
        )
        .branch(
            dptree::filter(|msg: Message| {
                msg.photo().is_some() || msg.document().is_some() || msg.sticker().is_some()
            })
            .endpoint(handle_file),
        );

    // 启动机器人
    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
