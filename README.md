# nport

nport is a tool for exposing your local ports or servers on the Internet.

## Quick Start

To install `np`, use one of the static binaries provided in the releases. By default, `np` uses
the API at `api.nport.io` to expose your ports. Use `np -h` to see the available options.

```
Usage: np [OPTIONS] [COMMAND]

Commands:
  http
  tcp
  version
  help     Print this message or the help of the given subcommand(s)

Options:
  -H, --hostname <HOSTNAME>
  -R, --remote-port <REMOTE_PORT>
  -N, --no-config-file
  -h, --help                       Print help
```

## Config file

`np` looks for a file named `nport.{yaml,json,ini}` in the current directory, with the following format:

```yaml
tunnels:
  - type: http
    # This assigns my-hostname.nport.io to your HTTP tunnel. If empty, a random hostname is generated
    hostname: my-hostname
    local_addr: 8089 # Just a port number indicates 127.0.0.1:<port>, ip:port syntax is also supported

  - type: tcp
    remote_addr: tcp.nport.io:9999 # Leave the host empty to assign the default port
    local_addr: 192.168.1.7:3000

server:
  hostname: api.nport.io # API server host, defaults to api.nport.io
  secure: true # Use TLS when talking to the server, defaults to true
```

## Running your own server

See [nport-server/deploy](nport-server/deploy) for an example deployment setup to run your own server.

## Reporting bugs

When opening an issue, make sure you attach a log running `np` with TRACE logging enabled.
Use `RUST_LOG='np=trace' np ...` to do so.
