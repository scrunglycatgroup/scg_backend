# syntax=docker/dockerfile:1
FROM rust:1.67

WORKDIR /usr/src/scg_backend
COPY . . 

# Build release
RUN cargo install --path .

# Run 
CMD ["scg_backend"]
