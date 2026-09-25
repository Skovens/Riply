FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN source $HOME/.cargo/env && rustup component add clippy rustfmt

# Install Node.js
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
RUN apt-get update && apt-get install -y nodejs

# Install SQLite dev dependencies
RUN apt-get update && apt-get install -y libsqlite3-dev

# Install pre-commit
RUN pip install pre-commit

# Set up PATH
ENV PATH="/root/.cargo/bin:${PATH}"
