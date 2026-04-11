use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use dotenv::dotenv;
use teloxide::prelude::*;
use teloxide::types::ChatId;

mod handlers;
mod processors;
mod state;

use handlers::{
    BotCommand, command_handler, handle_file, unauthorized_access_handler,
    unhandled_message_handler,
};
use state::ModeState;

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    dotenv().ok();

    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting Telegram sticker bot...");

    // 从环境变量获取机器人TOKEN
    let token = std::env::var("TELEGRAM_BOT_TOKEN")
        .context("未找到TELEGRAM_BOT_TOKEN环境变量。请在.env文件中设置或直接设置环境变量")?;

    // 白名单逻辑
    let allowed_chat_ids_opt: Option<Arc<Vec<ChatId>>> = match std::env::var("ALLOWED_CHAT_IDS") {
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

    // 初始化模式状态
    let mode_state: ModeState = Arc::new(Mutex::new(HashMap::new()));

    // 为命令处理程序创建过滤器闭包
    let command_filter_ids_clone = allowed_chat_ids_opt.clone();
    let command_auth_filter = move |msg: Message| match &command_filter_ids_clone {
        Some(allowed_ids) => allowed_ids.contains(&msg.chat.id),
        None => true,
    };

    // 为文件处理程序创建过滤器闭包
    let file_filter_ids_clone = allowed_chat_ids_opt.clone();
    let file_auth_and_type_filter = move |msg: Message| {
        let authorized = match &file_filter_ids_clone {
            Some(allowed_ids) => allowed_ids.contains(&msg.chat.id),
            None => true,
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

    // 为未处理消息创建认证过滤器
    let unhandled_message_auth_filter_ids = allowed_chat_ids_opt.clone();
    let unhandled_message_auth_filter = move |msg: Message| match &unhandled_message_auth_filter_ids
    {
        Some(allowed_ids) => allowed_ids.contains(&msg.chat.id),
        None => true,
    };

    // 克隆 mode_state 用于各个分支
    let mode_state_cmd = mode_state.clone();
    let mode_state_file = mode_state.clone();
    let mode_state_unhandled = mode_state.clone();

    // 创建处理器
    let handler = Update::filter_message()
        .branch(
            dptree::entry()
                .filter_command::<BotCommand>()
                .filter(command_auth_filter)
                .endpoint(move |bot: Bot, cmd: BotCommand, msg: Message| {
                    let mode_state = mode_state_cmd.clone();
                    async move { command_handler(bot, msg, cmd, mode_state).await }
                }),
        )
        .branch(dptree::filter(file_auth_and_type_filter).endpoint(
            move |bot: Bot, msg: Message| {
                let mode_state = mode_state_file.clone();
                async move { handle_file(bot, msg, mode_state).await }
            },
        ))
        .branch(
            dptree::entry()
                .filter(unhandled_message_auth_filter)
                .endpoint(move |bot: Bot, msg: Message| {
                    let mode_state = mode_state_unhandled.clone();
                    async move { unhandled_message_handler(bot, msg, mode_state).await }
                }),
        )
        .branch(dptree::endpoint(unauthorized_access_handler));

    // 启动机器人
    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
