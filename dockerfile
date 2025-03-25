# syntax=docker/dockerfile:1
FROM rust:1.85 AS build  # change from rust build source 

WORKDIR /usr/src/scg_backend
COPY . . 

# Build release
RUN cargo install --path .


## Create the runner build 
FROM docker.io/debian:bookworm-slim

RUN apt-get update && apt-get install libssl-dev -y

COPY --from=build /usr/local/cargo/bin/scg_backend /usr/local/bin/scg_backend

ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8080

#RUN
CMD ["scg_backend"]
