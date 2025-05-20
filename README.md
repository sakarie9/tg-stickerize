# Telegram Sticker Optimizer Bot

[English](./README_EN.md) | 中文

这是一个Telegram机器人，可以将用户发送的图片和WebM视频处理成符合Telegram贴纸要求的格式。

## 功能

- **图片处理**:
  - 将图片调整为一边为512像素，另一边按比例缩放。
  - 将图片转换为WebP格式。
  - 确保处理后的图片文件大小不超过512KB。
- **视频处理 (WebM)**:
  - 将视频调整为一边为512像素，另一边按比例缩放。
  - 视频时长限制在3秒以内。
  - 视频帧率限制在30fps以内。
  - 将视频转换为VP9编码的WebM格式。
  - 确保处理后的视频文件大小不超过256KB。
- **支持的输入**:
  - 图片 (JPEG, PNG, GIF等，由 `image` crate支持的格式)
  - 视频 (WebM, MP4等，由 `ffmpeg` 支持的格式，但主要针对WebM优化)
  - Telegram贴纸 (图片或视频类型)
  - Telegram动图 (通常是MP4格式)
- **自动类型检测**: 使用 `infer` 库检测文件类型，即使Telegram没有提供准确的MIME类型。

## 安装与配置

### 先决条件

- **Rust**: [安装Rust](https://www.rust-lang.org/tools/install)
- **FFmpeg 和 FFprobe**: 用于视频处理。
  - 在Debian/Ubuntu上: `sudo apt update && sudo apt install ffmpeg`
  - 在macOS上 (使用Homebrew): `brew install ffmpeg`
  - 在Windows上: 从 [FFmpeg官网](https://ffmpeg.org/download.html) 下载并将其添加到PATH。
- **libvpx**: FFmpeg通常会自带，但如果遇到VP9编码问题，请确保已安装。
  - 在Debian/Ubuntu上: `sudo apt install libvpx-dev`

### 步骤

1. **克隆仓库**:

    ```bash
    git clone <repository-url>
    cd tg-stickerize
    ```

2. **创建 `.env` 文件**:
    在项目根目录下创建一个名为 `.env` 的文件，并添加你的Telegram机器人Token：

    ```env
    TELEGRAM_BOT_TOKEN=your_telegram_bot_token_here
    ```

    你可以从BotFather获取机器人Token。

3. **编译项目**:

    ```bash
    cargo build --release
    ```

## 运行机器人

### 从二进制运行

从 release 中下载对应系统架构的二进制文件运行

### 从源码运行

```bash
cargo run --release
```

机器人启动后，它会开始监听来自Telegram的消息。

## 使用方法

1. 在Telegram中找到你的机器人。
2. 向机器人发送图片或WebM视频文件。
    - 可以直接发送图片文件。
    - 可以直接发送视频文件 (推荐WebM，其他格式会尝试转换)。
    - 可以发送现有的贴纸或动图。
3. 机器人会自动处理文件，并回复一个优化后的贴纸文件。
4. 你可以将机器人回复的贴纸文件转发给 `@Stickers` 机器人，并按照提示将其添加到你的贴纸包中。

### 支持的命令

- `/start` - 显示欢迎信息和使用说明。
- `/help` - 显示帮助信息和使用说明。

## 注意事项

- 视频处理依赖于外部的 `ffmpeg` 和 `ffprobe` 命令。请确保它们已正确安装并在系统的PATH中。
- 图片处理使用 `image` crate，视频处理使用 `ffmpeg` 进行转换和调整。
- 文件大小限制是根据Telegram对贴纸的要求设定的。
