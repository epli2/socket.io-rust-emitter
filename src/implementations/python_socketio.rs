use std::fmt::Display;

use redis::Commands;
use serde::Serialize;
use serde_json::json;

use crate::Emitter;

impl Emitter {
    /// Overrides the default channel name.
    pub fn channel(mut self, name: &str) -> Emitter {
        self.channel = name.to_string();
        self
    }

    pub fn emit_json<T: Serialize, Event: Display>(mut self, event: Event, message: T) -> Emitter {
        fn _emit_json(
            redis: &mut redis::Client,
            channel: &str,
            nsp: &str,
            room: Option<&str>,
            event: &str,
            data: &serde_json::Value,
        ) {
            let message = json!({
                "method": "emit",
                "event": event,
                "data": data,
                "namespace": nsp,
                "room": room,
                "skip_sid": null,
                "callback": null,
                "host_id": null,
            });
            let _: () = redis.publish(channel, message.to_string()).unwrap();
        }
        let event = event.to_string();
        let data = serde_json::to_value(&message).unwrap();

        if self.rooms.is_empty() {
            _emit_json(
                &mut self.redis,
                &self.channel,
                &self.nsp,
                None,
                &event,
                &data,
            );
        } else {
            for room in self.rooms.iter() {
                _emit_json(
                    &mut self.redis,
                    &self.channel,
                    &self.nsp,
                    Some(&room),
                    &event,
                    &data,
                );
            }
        }
        self
    }
}

#[cfg(test)]
mod test {
    use std::{io::Write, time::Duration};

    use super::*;
    use serde_json::json;
    use testcontainers::{core::ExecCommand, runners::SyncRunner, ImageExt};

    struct PythonSocketIOImage {
        mounts: Vec<testcontainers::core::Mount>,
        // Keep temp file alive
        _temp_file: tempfile::NamedTempFile,
    }

    impl PythonSocketIOImage {
        fn new(redis_url: String, channel: Option<String>) -> Self {
            let mut temp_file = tempfile::NamedTempFile::new().unwrap();
            let channel_python_arg = if let Some(channel) = channel {
                format!(",channel='{}'", channel)
            } else {
                "".to_owned()
            };
            temp_file
                .as_file_mut()
                .write_all(
                    b"
import socketio
import os
import logging
import asyncio

logging.basicConfig(level=logging.DEBUG)

",
                )
                .unwrap();

            temp_file
                .as_file_mut()
                .write_all(
                    format!(
                        r#"client_manager = socketio.AsyncRedisManager(url="{}"{})"#,
                        redis_url, channel_python_arg
                    )
                    .as_bytes(),
                )
                .unwrap();

            temp_file
                .as_file_mut()
                .write_all(
                    b"
loop = asyncio.new_event_loop()

print('Server running')
with open('/var/run/socketio.ready', 'w') as f:
    f.write('ready')
while True:
    # We need to flush in order to see the message across the pipe
    print('message:', loop.run_until_complete(client_manager._listen()), flush=True)
",
                )
                .unwrap();
            let path = temp_file.path().to_str().unwrap().to_string();
            Self {
                mounts: vec![testcontainers::core::Mount::bind_mount(
                    path,
                    "/opt/socketio/server.py",
                )],
                _temp_file: temp_file,
            }
        }
    }

    impl PythonSocketIOImage {
        fn messages(container: &testcontainers::Container<Self>) -> Vec<String> {
            // Grab the contents of /var/log/socketio.log
            let mut buf = String::new();
            container
                .exec(
                    ExecCommand::new(vec!["cat", "/var/log/socketio.log"])
                        .with_cmd_ready_condition(testcontainers::core::CmdWaitFor::exit_code(0)),
                )
                .unwrap()
                .stdout()
                .read_to_string(&mut buf)
                .unwrap();
            // Unfortunately this log message doesn't have the contents of the message
            buf.lines()
                .filter_map(|line| {
                    if line.starts_with("message: ") {
                        Some(line[("message: ".len())..].to_string())
                    } else {
                        None
                    }
                })
                .collect()
        }
    }

    impl testcontainers::Image for PythonSocketIOImage {
        fn name(&self) -> &str {
            "python"
        }

        fn tag(&self) -> &str {
            // This is the last supported release of aioredis (which is a dependency of python-socketio)
            "3.10"
        }

        fn ready_conditions(&self) -> Vec<testcontainers::core::WaitFor> {
            vec![]
        }

        fn mounts(&self) -> impl IntoIterator<Item = &testcontainers::core::Mount> {
            self.mounts.iter()
        }

        fn expose_ports(&self) -> &[testcontainers::core::ContainerPort] {
            &[testcontainers::core::ContainerPort::Tcp(8443)]
        }

        fn cmd(&self) -> impl IntoIterator<Item = impl Into<std::borrow::Cow<'_, str>>> {
            // Sleep for infinity, we will start the server in the test
            vec!["sleep", "infinity"].into_iter()
        }
    }

    fn launch_containers(
        channel: Option<String>,
    ) -> (
        testcontainers::Container<testcontainers::GenericImage>,
        String,
        testcontainers::Container<PythonSocketIOImage>,
    ) {
        let redis = crate::tests::launch_redis_container();
        std::thread::sleep(Duration::from_secs(1));

        // Get the first 12 characters of the container id, this is the hostname
        let container_redis_url = format!("redis://{}:{}/0", &redis.id()[..12], 6379);
        let python_socketio_server = PythonSocketIOImage::new(container_redis_url.clone(), channel)
            .with_network(crate::tests::DOCKER_NETWORK_NAME)
            .start()
            .unwrap();
        python_socketio_server
            .exec(
                ExecCommand::new(vec!["pip", "install", "python-socketio==4.6.1", "python-engineio==3.14.2", "six==1.16.0", "aioredis==1.3.1"])
                    .with_cmd_ready_condition(testcontainers::core::CmdWaitFor::message_on_stdout(
                        "Successfully installed aioredis-1.3.1 async-timeout-4.0.3 hiredis-3.0.0 python-engineio-3.14.2 python-socketio-4.6.1 six-1.16.0\n",
                    )),
            )
            .unwrap();
        python_socketio_server
            .exec(ExecCommand::new(vec![
                "/bin/bash",
                "-c",
                "(python /opt/socketio/server.py 2>&1) > /var/log/socketio.log",
            ]))
            .unwrap();
        // Wait for the server to start by checking the ready file
        python_socketio_server
            .exec(
                ExecCommand::new(vec![
                    "/bin/sh",
                    "-c",
                    "while [ ! -f /var/run/socketio.ready ]; do sleep 1; done",
                ])
                .with_cmd_ready_condition(testcontainers::core::CmdWaitFor::exit_code(0)),
            )
            .unwrap();

        let host_redis_url = format!(
            "redis://localhost:{}/0",
            redis.get_host_port_ipv4(6379).unwrap()
        );
        (redis, host_redis_url, python_socketio_server)
    }

    #[test]
    fn test_emit_json() {
        let (redis, redis_url, container) = launch_containers(None);
        let emitter = Emitter::new(redis::Client::open(redis_url).unwrap());
        emitter
            .to("room")
            .emit_json("my_event", json!({"key": "value"}));
        // Now check to see if the message was received
        let messages = PythonSocketIOImage::messages(&container);
        container.stop().unwrap();
        redis.stop().unwrap();
        container.rm().unwrap();
        redis.rm().unwrap();
        assert_eq!(
            messages,
            vec![
                r#"b'{"callback":null,"data":{"key":"value"},"event":"my_event","host_id":null,"method":"emit","namespace":"/","room":"room","skip_sid":null}'"#
            ]
        );
    }

    #[test]
    fn test_custom_channel() {
        let (redis, redis_url, container) = launch_containers(Some("custom_channel".to_string()));
        let emitter = Emitter::new(redis::Client::open(redis_url).unwrap()).channel("custom_channel");
        emitter
            .to("room")
            .emit_json("my_event", json!({"key": "value"}));
        // Now check to see if the message was received
        let messages = PythonSocketIOImage::messages(&container);
        container.stop().unwrap();
        redis.stop().unwrap();
        container.rm().unwrap();
        redis.rm().unwrap();
        assert_eq!(
            messages,
            vec![
                r#"b'{"callback":null,"data":{"key":"value"},"event":"my_event","host_id":null,"method":"emit","namespace":"/","room":"room","skip_sid":null}'"#
            ]
        );
    }

    #[test]
    fn test_no_room() {
        let (redis, redis_url, container) = launch_containers(None);
        let emitter = Emitter::new(redis::Client::open(redis_url).unwrap());
        emitter.emit_json("my_event", json!({"key": "value"}));
        // Now check to see if the message was received
        let messages = PythonSocketIOImage::messages(&container);
        container.stop().unwrap();
        redis.stop().unwrap();
        container.rm().unwrap();
        redis.rm().unwrap();
        assert_eq!(
            messages,
            vec![
                r#"b'{"callback":null,"data":{"key":"value"},"event":"my_event","host_id":null,"method":"emit","namespace":"/","room":null,"skip_sid":null}'"#
            ]
        );
    }

    #[test]
    fn test_array_data() {
        let (redis, redis_url, container) = launch_containers(None);
        let emitter = Emitter::new(redis::Client::open(redis_url).unwrap());
        emitter.emit_json("my_event", json!([1, 2, 3]));
        // Now check to see if the message was received
        let messages = PythonSocketIOImage::messages(&container);
        container.stop().unwrap();
        redis.stop().unwrap();
        container.rm().unwrap();
        redis.rm().unwrap();
        assert_eq!(
            messages,
            vec![
                r#"b'{"callback":null,"data":[1,2,3],"event":"my_event","host_id":null,"method":"emit","namespace":"/","room":null,"skip_sid":null}'"#
            ]
        );
    }
}
