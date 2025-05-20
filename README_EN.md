# Telegram Sticker Optimizer Bot

English | [中文](./README.md)

This is a Telegram bot that processes images and WebM videos sent by users into a format suitable for Telegram stickers.

## Features

- **Image Processing**:
  - Resizes images to have one side of 512 pixels, with the other side scaled proportionally.
  - Converts images to WebP format.
  - Ensures the processed image file size does not exceed 512KB.
- **Video Processing (WebM)**:
  - Resizes videos to have one side of 512 pixels, with the other side scaled proportionally.
  - Limits video duration to 3 seconds or less.
  - Limits video frame rate to 30fps or less.
  - Converts videos to VP9 encoded WebM format.
  - Ensures the processed video file size does not exceed 256KB.
- **Supported Inputs**:
  - Images (JPEG, PNG, GIF, etc., formats supported by the `image` crate)
  - Videos (WebM, MP4, etc., formats supported by `ffmpeg`, but primarily optimized for WebM)
  - Telegram stickers (image or video type)
  - Telegram animated GIFs (usually MP4 format)
- **Automatic Type Detection**: Uses the `infer` library to detect file types, even if Telegram doesn't provide an accurate MIME type.

## Installation and Configuration

### Prerequisites

- **Rust**: [Install Rust](https://www.rust-lang.org/tools/install)
- **FFmpeg and FFprobe**: For video processing.
  - On Debian/Ubuntu: `sudo apt update && sudo apt install ffmpeg`
  - On macOS (using Homebrew): `brew install ffmpeg`
  - On Windows: Download from the [FFmpeg official website](https://ffmpeg.org/download.html) and add it to your PATH.
- **libvpx**: Usually comes with FFmpeg, but ensure it's installed if you encounter VP9 encoding issues.
  - On Debian/Ubuntu: `sudo apt install libvpx-dev`

### Steps

1. **Clone the repository**:

    ```bash
    git clone <repository-url>
    cd tg-stickerize
    ```

2. **Create `.env` file**:
    Create a file named `.env` in the project root directory and add your Telegram bot token:

    ```env
    TELEGRAM_BOT_TOKEN=your_telegram_bot_token_here
    ```

    You can get your bot token from BotFather.

3. **Build the project**:

    ```bash
    cargo build --release
    ```

## Running the Bot

### Running from binary

Download the binary file for the corresponding system architecture from the release and run it.

### Running from source

```bash
cargo run --release
```

Once the bot starts, it will begin listening for messages from Telegram.

## How to Use

1. Find your bot on Telegram.
2. Send an image or WebM video file to the bot.
    - You can send image files directly.
    - You can send video files directly (WebM recommended, other formats will be attempted to convert).
    - You can send existing stickers or animated GIFs.
3. The bot will automatically process the file and reply with an optimized sticker file.
4. You can forward the sticker file replied by the bot to the `@Stickers` bot and follow the prompts to add it to your sticker pack.

### Supported Commands

- `/start` - Displays a welcome message and usage instructions.
- `/help` - Displays help information and usage instructions.

## Notes

- Video processing relies on external `ffmpeg` and `ffprobe` commands. Ensure they are correctly installed and in the system's PATH.
- Image processing uses the `image` crate, and video processing uses `ffmpeg` for conversion and adjustments.
- File size limits are set according to Telegram's requirements for stickers.
