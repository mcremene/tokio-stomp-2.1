# Use the official Rust image
FROM rust:1.81 AS builder

# Set the working directory in the container
WORKDIR /usr/src/app

# Copy the Cargo files to the container
COPY Cargo.toml ./
COPY . ./

# Pre-build dependencies (this helps with caching)
RUN cargo build --release --package tokio-stomp-2_1 --example connect_tls

# Stage 2: Prepare the runtime environment
FROM debian:bookworm-slim

# Install python3 and protobuf compiler in the runtime image
RUN apt-get update && \
    apt-get install -y python3 libpython3.11 libpq-dev ca-certificates && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /usr/src/app

# Copy the compiled Rust binary from the builder stage
COPY --from=builder /usr/src/app/target/release/examples/connect_tls /usr/local/bin/tokio-stomp-2_1

# Set the default command to run the service
CMD ["/usr/local/bin/tokio-stomp-2_1"]
