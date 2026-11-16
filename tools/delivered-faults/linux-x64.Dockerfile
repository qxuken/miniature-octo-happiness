FROM docker.io/library/rust:1.99.0-bookworm@sha256:b36c246742b4d323472588f789601be34eb4af053255e2af53969078ee8dcec7

RUN apt-get update \
    && apt-get install -y --no-install-recommends clang-16=1:16.0.6-15~deb12u1 \
    && apt-get clean \
    && find /var/lib/apt/lists -mindepth 1 -delete

ENV PATH="/usr/lib/llvm-16/bin:${PATH}"
RUN clang --version && findmnt --version
