FROM docker.io/library/rust:1.100.0-bookworm@sha256:198cd28b4ba92b20fe849d5d8b119ace1bd28423d9e82e9c4de5ee1300d453a0

RUN apt-get update \
    && apt-get install -y --no-install-recommends clang-16=1:16.0.6-15~deb12u1 \
    && apt-get clean \
    && find /var/lib/apt/lists -mindepth 1 -delete

ENV PATH="/usr/lib/llvm-16/bin:${PATH}"
RUN clang --version && findmnt --version
