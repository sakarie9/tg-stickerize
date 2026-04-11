use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use teloxide::types::ChatId;

/// 工作模式枚举
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// 贴纸优化模式：将图片/视频转为 Telegram 贴纸格式
    StickerOptimize,
    /// GIF下载模式：将视频转为 GIF 文件返回
    GifDownload,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::StickerOptimize => write!(f, "贴纸优化模式"),
            Mode::GifDownload => write!(f, "GIF下载模式"),
        }
    }
}

/// 聊天模式状态管理：每个 ChatId 对应一个 Mode
pub type ModeState = Arc<Mutex<HashMap<ChatId, Mode>>>;

/// 获取或初始化聊天的模式
pub fn get_chat_mode(mode_state: &ModeState, chat_id: ChatId) -> Mode {
    let mut modes = mode_state.lock().unwrap();
    *modes.entry(chat_id).or_insert(Mode::StickerOptimize)
}

/// 切换聊天模式
pub fn toggle_chat_mode(mode_state: &ModeState, chat_id: ChatId) -> Mode {
    let mut modes = mode_state.lock().unwrap();
    let current = modes.entry(chat_id).or_insert(Mode::StickerOptimize);
    *current = match *current {
        Mode::StickerOptimize => Mode::GifDownload,
        Mode::GifDownload => Mode::StickerOptimize,
    };
    *current
}
