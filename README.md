# Podman Socket Proxy

## Introduction
Podman Socket Proxy is designed to conceal sensitive APIs of the Podman engine from various services. This application allows proxying via TCP or Unix socket.

## Installation

### Docker

A container file is provided for easy deployment. To build and run the container, use the following commands:

```sh
podman build -t podman-socket-proxy .
podman run -d -p <host-port>:8787 podman-socket-proxy
```

It will proxy via TCP by default for ease of use inside container clusters. 

### Bare Metal

#### Installation

To build the project, ensure you have Rust installed and then run:

```bash
cargo build --release
```

#### Usage

```bash
podman-socket-proxy [OPTIONS] <COMMAND>
```

### Commands:
- `unix`  Start the proxy as a Unix Socket
- `inet`  Start the proxy as a TCP Socket
- `help`  Print this message or the help of the given subcommand(s)

### Options:
- `-p, --podman-path <PODMAN_PATH>`  The full path to the Podman socket [default: /run/podman/podman.sock]
- `-c, --config-path <CONFIG_PATH>`  The path to the TOML configuration file [default: ./config.toml]

#### Unix Usage

```bash
podman-socket-proxy unix [OPTIONS]
```

##### Options:
- `-s, --socket-path <SOCKET_PATH>`  The full path of the protected socket [default: /var/run/safe-podman.sock]
- `-r, --replace`                    Replace the socket file if it already exists

#### TCP Usage

```bash
podman-socket-proxy inet [OPTIONS]
```

##### Options:
- `-i, --ip <IP>`      The IP address the protected socket will listen on [default: 0.0.0.0]
- `-p, --port <PORT>`  The port the protected socket will listen on [default: 8787]

### Partial Docker Compatibility
While primarily designed for Podman, this proxy offers great compatibility with Docker. Although minimal support will be provided for Docker-related configurations.

## Dependencies

Podman Socket Proxy relies on several crates to function:

- anyhow = "1.0.93"
- clap = "4.5.21"
- clap_complete = "4.5.44"
- env_logger = "0.11.6"
- httparse = "1.9.5"
- lazy_static = "1.5.0"
- log = "0.4.25"
- regex = "1.11.1"
- serde = "1.0.215"
- thiserror = "2.0.3"
- tokio = "1.41.1"
- toml = "0.8.19"  

## Troubleshooting
If you encounter issues, check the following:
- Ensure that the Podman engine is running.
- Verify that the correct ports and socket paths are specified.
- Check the config file for syntax/regex errors.

## Contributing
Contributions are welcome! Please submit pull requests or open issues for any bugs or feature requests.

## License
This project is licensed under the GNU General Public License v3.0 
