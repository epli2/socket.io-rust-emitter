# socketio-rust-emitter

Emits messages to a Redis instance for consumption by either:
- (default) socket.io servers (protocol version 5, socket.io-redis-adapter v7) (under the feature flag `js_v7`)
- python-socketio (version 4) (under the feature flag `py_v4`)

[![build status](https://github.com/epli2/socketio-rust-emitter/actions/workflows/ci.yaml/badge.svg?branch=master&event=push)](https://github.com/epli2/socketio-rust-emitter/actions)
[![socketio-rust-emitter at crates.io](https://img.shields.io/crates/v/socketio-rust-emitter.svg)](https://crates.io/crates/socketio-rust-emitter)
[![socketio-rust-emitter at docs.rs](https://docs.rs/socketio-rust-emitter/badge.svg)](https://docs.rs/socketio-rust-emitter)

A Rust implementation of [socket.io-emitter](https://github.com/socketio/socket.io-emitter).

## How to use

```rust
use chrono::Utc;
use std::thread;
use std::time::Duration;

let io = Emitter::new("127.0.0.1");
let _ = thread::spawn(move || loop {
    thread::sleep(Duration::from_millis(5000));
    io.clone().emit(vec!["time", &format!("{}", Utc::now())]);
}).join();
```

```rust
// Different constructor options.

//1. Initialize with host:port string
let io = Emitter::new("localhost:6379")
// 2. Initlize with host, port object.
let io = Emitter::new(EmitterOpts {
    host: "localhost".to_owned(),
    port: 6379,
    ..Default::default()
});
```

## Examples

```rust
let io = Emitter::new(EmitterOpts { host: "127.0.0.1".to_owned(), port: 6379, ..Default::default() });

// sending to all clients
io.clone().emit(vec!["broadcast", /* ... */]);

// sending to all clients in "game" room
io.clone().to("game").emit(vec!["new-game", /* ... */]);

// sending to individual socketid (private message)
io.clone().to(<socketid>).emit(vec!["private", /* ... */]);

let nsp = io.clone().of("/admin");

// sending to all clients in "admin" namespace
nsp.clone().emit(vec!["namespace", /* ... */]);

// sending to all clients in "admin" namespace and in "notifications" room
nsp.clone().to("notifications").emit(vec!["namespace", /* ... */]);
```
