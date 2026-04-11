FROM jrottenberg/ffmpeg:7-scratch
ARG TARGETPLATFORM
COPY $TARGETPLATFORM/tg-stickerize /usr/bin/tg-stickerize
ENTRYPOINT [ "/usr/bin/tg-stickerize" ]
