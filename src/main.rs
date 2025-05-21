use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use dotenv::dotenv;
use image::imageops::FilterType;
use image::{GenericImageView, ImageReader};
use serde_json;
use teloxide::types::{ChatId, InputFile};
use teloxide::{prelude::*, utils::command::BotCommands};
use tempfile::{Builder, NamedTempFile};
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

async fn process_webm(input_path: &Path, output_path: &Path) -> Result<()> {
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
        "json", // 改用JSON格式输出
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
    let mut command = Command::new("ffmpeg");
    let status = command.args([
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
    ]);
    log::debug!("FFmpeg command: {:?}", &status);
    let status = status
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

async fn handle_file(bot: Bot, msg: Message) -> anyhow::Result<()> {
    log::info!("ChatID: {}, Received New message", msg.chat.id);

    let file_id = if let Some(photo) = msg.photo() {
        // 获取最高质量的照片，照片隐式为图片类型
        photo.last().expect("照片列表不应为空").file.id.clone()
    } else if let Some(document) = msg.document() {
        match document
            .mime_type
            .as_ref()
            .map(|mime| mime.to_string())
            .as_deref()
        {
            Some(mime)
                if mime.starts_with("image/")
                    || mime.starts_with("video/")
                    || mime == "application/octet-stream" =>
            {
                document.file.id.clone()
            }
            Some(mime) => {
                bot.send_message(
                    msg.chat.id,
                    format!("不支持的文档MIME类型: {}。请发送图片或WebM视频。", mime),
                )
                .await?;
                return Ok(());
            }
            None => {
                // 没有提供MIME类型，允许下载并让 infer 库决定
                document.file.id.clone()
            }
        }
    } else if let Some(sticker) = msg.sticker() {
        // 贴纸可以是 image/webp 或 video/webm。下载后让 infer 库确认类型。
        sticker.file.id.clone()
    } else if let Some(animation) = msg.animation() {
        match animation
            .mime_type
            .as_ref()
            .map(|mime| mime.to_string())
            .as_deref()
        {
            Some(mime) if mime.starts_with("video/") => {
                // 动画通常是视频 (例如 Telegram 中的 GIF 会作为 MP4 发送)
                animation.file.id.clone()
            }
            Some(mime) => {
                bot.send_message(
                    msg.chat.id,
                    format!("不支持的动画MIME类型: {}。请发送WebM视频。", mime),
                )
                .await?;
                return Ok(());
            }
            None => {
                // 没有提供MIME类型，允许下载并让 infer 库决定
                animation.file.id.clone()
            }
        }
    } else {
        bot.send_message(msg.chat.id, "请发送图片或WebM视频")
            .await?;
        return Ok(());
    };

    // 下载文件前先获取文件信息
    let tg_file = bot.get_file(&file_id).await?;

    // 创建输入临时文件
    let input_temp_file = NamedTempFile::new().context("无法创建输入临时文件")?;
    let input_file_path = input_temp_file.path().to_path_buf();

    // 下载文件
    let file_url = format!(
        "https://api.telegram.org/file/bot{}/{}",
        bot.token(),
        tg_file.path
    );
    let bytes = reqwest::get(&file_url).await?.bytes().await?;
    tokio_fs::write(&input_file_path, &bytes)
        .await
        .context("无法写入输入临时文件")?;

    // 检测文件类型并处理
    let detected_type_result =
        infer::get_from_path(&input_file_path).context("无法从路径获取类型信息推断")?;

    let (is_image, is_video, detected_mime_str) = match detected_type_result {
        Some(info) => (
            info.mime_type().starts_with("image/"),
            info.mime_type().starts_with("video/"),
            info.mime_type().to_string(),
        ),
        None => (false, false, "未知 (infer无法识别)".to_string()),
    };

    let processing_outcome: Result<(NamedTempFile, std::path::PathBuf), anyhow::Error>;

    if is_image {
        let output_temp = Builder::new()
            .suffix(".webp")
            .tempfile()
            .context("无法创建WebP输出临时文件")?;
        let output_path = output_temp.path().to_path_buf();

        log::debug!(
            "ChatID: {}, 输入: {:?}, 检测到的类型: {}. 输出到: {:?}",
            msg.chat.id,
            input_file_path,
            detected_mime_str,
            output_path
        );
        processing_outcome = process_image(&input_file_path, &output_path)
            .await
            .map(|_| (output_temp, output_path))
            .context("图片处理失败");
    } else if is_video {
        let output_temp = Builder::new()
            .suffix(".webm")
            .tempfile()
            .context("无法创建WebM输出临时文件")?;
        let output_path = output_temp.path().to_path_buf();

        log::debug!(
            "ChatID: {}, 输入: {:?}, 检测到的类型: {}. 输出到: {:?}",
            msg.chat.id,
            input_file_path,
            detected_mime_str,
            output_path
        );
        processing_outcome = process_webm(&input_file_path, &output_path)
            .await
            .map(|_| (output_temp, output_path))
            .context("视频处理失败");
    } else {
        processing_outcome = Err(anyhow!(
            "不支持的文件类型 (检测为: {}). 请发送图片或WebM视频.",
            detected_mime_str
        ));
    }

    // 处理结果
    match processing_outcome {
        Ok((_output_temp_file_guard, processed_file_path)) => {
            // _output_temp_file_guard 使临时文件保持活动状态
            // 发送处理后的文件
            let input_doc = InputFile::file(&processed_file_path);
            // 根据原始判断（is_image）或处理后的文件类型发送
            // 当前代码对图片和视频都使用 send_sticker
            bot.send_sticker(msg.chat.id, input_doc).await?;
            log::info!(
                "ChatID: {}, 处理成功，发送文件: {:?}",
                msg.chat.id,
                processed_file_path
            );
            // 可选：发送成功消息
            // bot.send_message(
            //     msg.chat.id,
            //     "这是处理后的贴纸，您可以添加到 @Stickers bot 创建的贴纸包中",
            // )
            // .await?;
        }
        Err(e) => {
            // 向用户发送一个简洁的错误消息
            bot.send_message(msg.chat.id, format!("处理失败: {}", e.root_cause())) // 使用 e.root_cause() 或 e.to_string() 获取更简洁的用户友好消息
                .await?;
            // 在控制台记录详细的错误信息，包括堆栈追踪（如果 RUST_BACKTRACE=1）
            log::error!("文件处理失败: {:?}", e);
        }
    }
    // input_temp_file 和 _output_temp_file_guard (如果Ok) 将在此处超出作用域，
    // 导致它们对应的临时文件被自动删除。
    Ok(())
}

async fn send_welcome_message(bot: Bot, chat_id: ChatId) -> anyhow::Result<()> {
    bot.send_message(
        chat_id,
        "欢迎使用 Telegram Sticker 优化工具！\n\n\
         请发送图片或WebM视频，我将处理成符合 Telegram 贴纸要求的格式。\n\n\
         - 图片将被调整至合适尺寸并转换为WebP格式\n\
         - 视频将被调整至合适尺寸、帧率和时长，转换为WebM格式\n\n\
         您也可以使用 /help 查看可用命令。\n\n\
         处理后，您可以将文件添加到 @Stickers bot 创建的贴纸包中。",
    )
    .await?;
    Ok(())
}

async fn command_handler(bot: Bot, msg: Message, cmd: BotCommand) -> anyhow::Result<()> {
    match cmd {
        BotCommand::Help | BotCommand::Start => {
            send_welcome_message(bot, msg.chat.id).await?;
        }
    }
    Ok(())
}

async fn unhandled_message_handler(bot: Bot, msg: Message) -> anyhow::Result<()> {
    send_welcome_message(bot, msg.chat.id).await?;
    Ok(())
}

async fn unauthorized_access_handler(bot: Bot, msg: Message) -> anyhow::Result<()> {
    log::warn!(
        "ChatID: {} - Unauthorized access attempt.",
        msg.chat.id
    );
    bot.send_message(msg.chat.id, "抱歉，您未被授权使用此机器人。")
        .await?;
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

    // 白名单逻辑
    let allowed_chat_ids_opt: Option<Arc<Vec<ChatId>>> =
        match std::env::var("ALLOWED_CHAT_IDS") {
            Ok(ids_str) => {
                let ids: Vec<ChatId> = ids_str
                    .split(',')
                    .filter_map(|id_str| id_str.trim().parse::<i64>().ok().map(ChatId))
                    .collect();
                
                if ids.is_empty() {
                    log::warn!(
                        "ALLOWED_CHAT_IDS 设置为 '{}', 解析后白名单为空。机器人将不会授权任何用户。",
                        ids_str
                    );
                } else {
                    log::info!("白名单已启用。允许的聊天 ID: {:?}", ids);
                }
                Some(Arc::new(ids))
            }
            Err(_) => {
                log::info!("未设置 ALLOWED_CHAT_IDS。机器人将响应所有用户。");
                None
            }
        };

    let bot = Bot::new(token);

    // 为命令处理程序创建过滤器闭包
    let command_filter_ids_clone = allowed_chat_ids_opt.clone();
    let command_auth_filter = move |msg: Message| {
        match &command_filter_ids_clone {
            Some(allowed_ids) => {
                if allowed_ids.contains(&msg.chat.id) {
                    true
                } else {
                    // log::warn!("ChatID: {} - 来自非白名单用户的未授权命令尝试。", msg.chat.id); // 日志将由 unauthorized_access_handler 处理
                    false
                }
            }
            None => true, // 没有白名单，允许所有用户
        }
    };

    // 为文件处理程序创建过滤器闭包
    let file_filter_ids_clone = allowed_chat_ids_opt.clone();
    let file_auth_and_type_filter = move |msg: Message| {
        let authorized = match &file_filter_ids_clone {
            Some(allowed_ids) => {
                if allowed_ids.contains(&msg.chat.id) {
                    true
                } else {
                    // log::warn!("ChatID: {} - 来自非白名单用户的未授权文件提交。", msg.chat.id); // 日志将由 unauthorized_access_handler 处理
                    false
                }
            }
            None => true, // 没有白名单，允许所有用户
        };

        if !authorized {
            return false;
        }

        // 原始文件类型检查
        msg.photo().is_some()
            || msg.document().is_some()
            || msg.sticker().is_some()
            || msg.animation().is_some()
    };

    // 为未处理消息创建认证过滤器 (用于 unhandled_message_handler)
    let unhandled_message_auth_filter_ids = allowed_chat_ids_opt.clone();
    let unhandled_message_auth_filter = move |msg: Message| {
        match &unhandled_message_auth_filter_ids {
            Some(allowed_ids) => allowed_ids.contains(&msg.chat.id),
            None => true, // 如果没有白名单，则所有人都被授权接收欢迎消息
        }
    };

    // 创建处理器
    let handler = Update::filter_message()
        .branch(
            dptree::entry()
                .filter_command::<BotCommand>()
                .filter(command_auth_filter) // 应用白名单过滤器
                .endpoint(command_handler),
        )
        .branch(
            dptree::filter(file_auth_and_type_filter) // 应用白名单和类型过滤器
            .endpoint(handle_file),
        )
        .branch( // 对于已授权用户发送的、非命令且非文件的消息
            dptree::entry()
                .filter(unhandled_message_auth_filter)
                .endpoint(unhandled_message_handler), // 发送欢迎信息
        )
        .branch(dptree::endpoint(unauthorized_access_handler)); // 对于未授权用户的任何其他消息

    // 启动机器人
    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
