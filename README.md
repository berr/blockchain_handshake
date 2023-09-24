# Blockchain Handshake

This program connects to a remote host running a blockchain node, runs the initial setup of a connection, sends a ping message and disconnects.

The handshake only works with clients of version 70016.


## Requirements
- Rustc version at least `1.70.0`, if compiling locally.
- `Docker`and `Docker Compose`, otherwise.

## Running

For simplicity there is a [docker-compose.yml]() file for running the project.

### Testing against a local blockchain

Run: `docker compose run local_handshake`

The handshake ran successfully if the program's return code is success, and the following outputted.

```
$ docker compose run handshake
Starting handshake
Handshake finished
Sending a ping to make sure we're correctly connected
Success!
Closing up the application
```

### Running unit tests
`$ docker compose run tests`

### Real blockchain nodes
You can also connect to a real blockchain node, using one from the list
https://bitnodes.io/nodes/?q=Satoshi:25.0.0.

For example:
```
$ docker compose run handshake 191.9.47.106
Starting handshake
Handshake finished
Sending a ping to make sure we're correctly connected
Success!
Closing up the application
```

### Debugging transmitted messages
For printing what messages are being received/sent through the network there is also:
- `docker compose run local_handshake_debug`
- `docker compose run handshake_debug 10.11.12.13`

### Command line arguments
It is also possible to change the target's host port, the local ip and port, user agent and timeout settings.
Run `docker compose run handshake -h` to see exactly how to change those.

### Local development
For local development, you can use the local blockchain, starting it with `docker compose up blockchain_node`.
To test against it, you can run `cargo run 127.0.0.1`.