FROM debian:stable-slim
COPY tg-stickerize /usr/bin/tg-stickerize
ENTRYPOINT [ "/usr/bin/tg-stickerize" ]