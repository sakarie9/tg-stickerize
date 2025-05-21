FROM jrottenberg/ffmpeg:7-scratch
COPY tg-stickerize /usr/bin/tg-stickerize
ENTRYPOINT [ "/usr/bin/tg-stickerize" ]
