FROM debian:latest

WORKDIR /root
RUN apt update && apt install -y wget
RUN wget https://bitcoin.org/bin/bitcoin-core-25.0/bitcoin-25.0-x86_64-linux-gnu.tar.gz -O bitcoin.tar.gz && \
    tar -xf bitcoin.tar.gz --strip=1
