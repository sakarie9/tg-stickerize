use anyhow::Context;
use teloxide::prelude::*;
use teloxide::types::ChatId;
use teloxide::utils::command::BotCommands;
use tempfile::Builder;
use tokio::fs as tokio_fs;

use crate::processors::{process_image, process_video_to_gif, process_webm};
use crate::state::{Mode, ModeState, get_chat_mode, toggle_chat_mode};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "支持的命令：")]
pub enum BotCommand {
    #[command(description = "显示此帮助信息")]
    Help,
    #[command(description = "开始使用bot")]
    Start,
    #[command(description = "切换工作模式")]
    Mode,
}

pub async fn send_welcome_message(bot: Bot, chat_id: ChatId, mode: Mode) -> anyhow::Result<()> {
    let mode_info = match mode {
        Mode::StickerOptimize => {
            "📦 **贴纸优化模式**\n\
            将图片和视频转为 Telegram 贴纸格式\n\
            - 图片 → WebP 贴纸\n\
            - 视频 → VP9 WebM 贴纸"
        }
        Mode::GifDownload => {
            "🎞️ **GIF 下载模式**\n\
            将视频转为 GIF 文件返回\n\
            - 视频 → GIF\n\
            - 动态贴纸 → GIF\n\
            - 动图 → GIF\n\
            - 图片 → 作为文档发送"
        }
    };

    let message = format!(
        "欢迎使用 Telegram Sticker 工具！\n\n\
        **当前模式**: {}\n\n\
        {}\n\n\
        使用 /mode 切换工作模式。",
        mode, mode_info
    );

    bot.send_message(chat_id, message).await?;
    Ok(())
}

pub async fn command_handler(
    bot: Bot,
    msg: Message,
    cmd: BotCommand,
    mode_state: ModeState,
) -> anyhow::Result<()> {
    match cmd {
        BotCommand::Help | BotCommand::Start => {
            let mode = get_chat_mode(&mode_state, msg.chat.id);
            send_welcome_message(bot, msg.chat.id, mode).await?;
        }
        BotCommand::Mode => {
            let new_mode = toggle_chat_mode(&mode_state, msg.chat.id);
            let extra = match new_mode {
                Mode::StickerOptimize => "现在可以发送图片或视频，我将处理成贴纸格式。",
                Mode::GifDownload => "现在可以发送视频、动图、动态贴纸或图片，我将返回 GIF 文件或原图。",
            };
            let message = format!("✅ 已切换到 **{}**\n\n{}", new_mode, extra);
            bot.send_message(msg.chat.id, message).await?;
        }
    }
    Ok(())
}

pub async fn unhandled_message_handler(
    bot: Bot,
    msg: Message,
    mode_state: ModeState,
) -> anyhow::Result<()> {
    let mode = get_chat_mode(&mode_state, msg.chat.id);
    send_welcome_message(bot, msg.chat.id, mode).await?;
    Ok(())
}

pub async fn unauthorized_access_handler(bot: Bot, msg: Message) -> anyhow::Result<()> {
    log::warn!("ChatID: {} - Unauthorized access attempt.", msg.chat.id);
    bot.send_message(msg.chat.id, "抱歉，您未被授权使用此机器人。")
        .await?;
    Ok(())
}

pub async fn handle_file(bot: Bot, msg: Message, mode_state: ModeState) -> anyhow::Result<()> {
    log::info!("ChatID: {}, Received New message", msg.chat.id);

    let file_id = if let Some(photo) = msg.photo() {
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
            None => document.file.id.clone(),
        }
    } else if let Some(sticker) = msg.sticker() {
        sticker.file.id.clone()
    } else if let Some(animation) = msg.animation() {
        match animation
            .mime_type
            .as_ref()
            .map(|mime| mime.to_string())
            .as_deref()
        {
            Some(mime) if mime.starts_with("video/") => animation.file.id.clone(),
            Some(mime) => {
                bot.send_message(
                    msg.chat.id,
                    format!("不支持的动画MIME类型: {}。请发送WebM视频。", mime),
                )
                .await?;
                return Ok(());
            }
            None => animation.file.id.clone(),
        }
    } else {
        bot.send_message(msg.chat.id, "请发送图片或WebM视频")
            .await?;
        return Ok(());
    };

    // 下载文件
    let tg_file = bot.get_file(file_id).await?;
    let input_temp_file = tempfile::NamedTempFile::new().context("无法创建输入临时文件")?;
    let input_file_path = input_temp_file.path().to_path_buf();

    let file_url = format!(
        "https://api.telegram.org/file/bot{}/{}",
        bot.token(),
        tg_file.path
    );
    let bytes = reqwest::get(&file_url).await?.bytes().await?;
    tokio_fs::write(&input_file_path, &bytes)
        .await
        .context("无法写入输入临时文件")?;

    // 检测文件类型
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

    // 获取当前模式
    let current_mode = get_chat_mode(&mode_state, msg.chat.id);
    log::info!("ChatID: {}, 当前模式: {:?}", msg.chat.id, current_mode);

    // GIF 模式下直接发送图片作为文档
    if current_mode == Mode::GifDownload && is_image {
        let input_doc = teloxide::types::InputFile::file(&input_file_path);
        bot.send_document(msg.chat.id, input_doc)
            .disable_content_type_detection(true)
            .await?;
        return Ok(());
    }

    // GIF 模式下不支持非视频文件
    if current_mode == Mode::GifDownload && !is_video {
        bot.send_message(
            msg.chat.id,
            format!(
                "不支持的文件类型 (检测为: {})。请发送视频、动图或动态贴纸。",
                detected_mime_str
            ),
        )
        .await?;
        return Ok(());
    }

    // 处理输出
    let processing_outcome: anyhow::Result<((tempfile::NamedTempFile, std::path::PathBuf), bool)>;

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
            .map(|_| ((output_temp, output_path), true))
            .context("图片处理失败");
    } else if is_video {
        if current_mode == Mode::StickerOptimize {
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
                .map(|_| ((output_temp, output_path), true))
                .context("视频处理失败");
        } else {
            // GIF 模式
            let output_temp = Builder::new()
                .suffix(".gif")
                .tempfile()
                .context("无法创建GIF输出临时文件")?;
            let output_path = output_temp.path().to_path_buf();
            log::debug!(
                "ChatID: {}, 输入: {:?}, 检测到的类型: {}. 输出GIF到: {:?}",
                msg.chat.id,
                input_file_path,
                detected_mime_str,
                output_path
            );
            processing_outcome = process_video_to_gif(&input_file_path, &output_path)
                .await
                .map(|_| ((output_temp, output_path), false))
                .context("GIF转换失败");
        }
    } else {
        processing_outcome = Err(anyhow::anyhow!(
            "不支持的文件类型 (检测为: {}). 请发送图片或WebM视频.",
            detected_mime_str
        ));
    }

    // 处理结果
    match processing_outcome {
        Ok(((_output_temp_file_guard, processed_file_path), is_sticker)) => {
            let input_doc = teloxide::types::InputFile::file(&processed_file_path);
            if is_sticker {
                bot.send_sticker(msg.chat.id, input_doc).await?;
            } else {
                bot.send_document(msg.chat.id, input_doc)
                    .disable_content_type_detection(true)
                    .await?;
            }
            log::info!(
                "ChatID: {}, 处理成功，发送文件: {:?}",
                msg.chat.id,
                processed_file_path
            );
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("处理失败: {}", e.root_cause()))
                .await?;
            log::error!("文件处理失败: {:?}", e);
        }
    }

    Ok(())
}
