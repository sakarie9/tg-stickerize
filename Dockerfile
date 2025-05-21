FROM alpine:latest
COPY tg-stickerize /usr/bin/tg-stickerize
ENTRYPOINT [ "/usr/bin/tg-stickerize" ]