FROM nvcr.io/nvidia/l4t-cuda:12.2.12-devel

# Fix GPG key issues in the base image, then install build dependencies.
# The L4T base images sometimes ship with corrupted/outdated GPG keys.
# Install packages one at a time to avoid disk space issues
RUN set -eux; \
    apt-get clean; rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*; \
    export DEBIAN_FRONTEND=noninteractive; \
    # Remove potentially corrupted keyrings
    rm -f /etc/apt/trusted.gpg.d/*.gpg; \
    # Install gnupg and ca-certificates without signature verification
    apt-get update --allow-insecure-repositories || true; \
    apt-get install -y --no-install-recommends --allow-unauthenticated -o APT::Keep-Downloaded-Packages="false" \
        gnupg; \
    apt-get clean; rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*; \
    apt-get update --allow-insecure-repositories || true; \
    apt-get install -y --no-install-recommends --allow-unauthenticated -o APT::Keep-Downloaded-Packages="false" \
        ca-certificates; \
    # Import fresh Ubuntu keyring
    apt-key adv --keyserver keyserver.ubuntu.com --recv-keys 871920D1991BC93C 3B4FE6ACC0B21F32 40976EAF437D05B5; \
    rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*; \
    # Now apt-get update should work properly
    apt-get update; \
    apt-get install -y --no-install-recommends \
        curl \
        build-essential \
        clang \
        pkg-config \
        git \
        libclang-dev \
        python3; \
    # Final cleanup
    apt-get clean; rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# ARM64 Jetson target
RUN rustup target add aarch64-unknown-linux-gnu
