#[macro_use]
extern crate serde_derive;

use redis::Commands;
use rmp_serde::Serializer;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Emitter {
    redis: redis::Client,
    prefix: String,
    nsp: String,
    channel: String,
    rooms: Vec<String>,
    flags: HashMap<String, bool>,
    uid: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Opts {
    rooms: Vec<String>,
    flags: HashMap<String, bool>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Packet {
    #[serde(rename = "type")]
    _type: i32,
    data: Vec<String>,
    nsp: String,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct EmitterOpts<'a> {
    pub host: String,
    pub port: i32,
    pub socket: Option<String>,
    pub key: Option<&'a str>,
}

pub trait IntoEmitter {
    fn into_emitter(self) -> Emitter;
}

impl IntoEmitter for redis::Client {
    fn into_emitter(self) -> Emitter {
        create_emitter(self, "socket.io", "/")
    }
}

impl<'a> IntoEmitter for EmitterOpts<'a> {
    fn into_emitter(self) -> Emitter {
        let addr = format!("redis://{}:{}", self.host, self.port);
        let prefix = self.key.unwrap_or("socket.io");

        create_emitter(redis::Client::open(addr.as_str()).unwrap(), prefix, "/")
    }
}

impl IntoEmitter for &str {
    fn into_emitter(self) -> Emitter {
        create_emitter(
            redis::Client::open(format!("redis://{}", self).as_str()).unwrap(),
            "socket.io",
            "/",
        )
    }
}

fn create_emitter(redis: redis::Client, prefix: &str, nsp: &str) -> Emitter {
    Emitter {
        redis,
        prefix: prefix.to_string(),
        nsp: nsp.to_string(),
        channel: format!("{}#{}#", prefix, nsp),
        rooms: Vec::new(),
        flags: HashMap::new(),
        uid: "emitter".to_string(),
    }
}

impl Emitter {
    pub fn new<I: IntoEmitter>(data: I) -> Emitter {
        data.into_emitter()
    }

    pub fn to(mut self, room: &str) -> Emitter {
        self.rooms.push(room.to_string());
        self
    }
    pub fn of(self, nsp: &str) -> Emitter {
        create_emitter(self.redis, self.prefix.as_str(), nsp)
    }
    pub fn json(mut self) -> Emitter {
        let mut flags = HashMap::new();
        flags.insert("json".to_string(), true);
        self.flags = flags;
        self
    }
    pub fn volatile(mut self) -> Emitter {
        let mut flags = HashMap::new();
        flags.insert("volatile".to_string(), true);
        self.flags = flags;
        self
    }
    pub fn broadcast(mut self) -> Emitter {
        let mut flags = HashMap::new();
        flags.insert("broadcast".to_string(), true);
        self.flags = flags;
        self
    }
    pub fn emit(mut self, message: Vec<&str>) -> Emitter {
        let packet = Packet {
            _type: 2,
            data: message.iter().map(|s| s.to_string()).collect(),
            nsp: self.nsp.clone(),
        };
        let opts = Opts {
            rooms: self.rooms.clone(),
            flags: self.flags.clone(),
        };
        let mut msg = Vec::new();
        let val = (self.uid.clone(), packet, opts);
        val.serialize(&mut Serializer::new(&mut msg).with_struct_map())
            .unwrap();

        let channel = if self.rooms.len() == 1 {
            format!("{}{}#", self.channel, self.rooms.join("#"))
        } else {
            self.channel.clone()
        };
        let _: () = self.redis.publish(channel, msg).unwrap();
        self.rooms = vec![];
        self.flags = HashMap::new();
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::{Emitter, Opts, Packet};
    use redis::Msg;
    use rmp_serde::Deserializer;
    use serde::Deserialize;
    use testcontainers::runners::SyncRunner;

    macro_rules! create_redis {
        ($redis:ident) => {
            let redis = testcontainers::GenericImage::new("redis", "latest")
                .with_exposed_port(testcontainers::core::ContainerPort::Tcp(6379))
                .with_wait_for(testcontainers::core::WaitFor::message_on_stdout("Ready to accept connections"))
                .start()
                .unwrap();
            let redis_url = format!(
                "redis://localhost:{}",
                redis.get_host_port_ipv4(6379).unwrap()
            );
            let $redis = redis::Client::open(redis_url.as_str()).unwrap();
        };
    }

    fn decode_msg(msg: Msg) -> (String, Packet, Opts) {
        let payload: Vec<u8> = msg.get_payload().unwrap();
        let mut de = Deserializer::new(&payload[..]);
        Deserialize::deserialize(&mut de).unwrap()
    }

    #[test]
    fn emit() {
        create_redis!(redis);
        let mut con = redis.get_connection().unwrap();
        let mut pubsub = con.as_pubsub();
        pubsub.subscribe("socket.io#/#").unwrap();

        // act
        let io = Emitter::new(redis);
        io.emit(vec!["test1", "test2"]);

        // assert
        let actual = decode_msg(pubsub.get_message().unwrap());
        assert_eq!("emitter", actual.0);
        assert_eq!(
            Packet {
                _type: 2,
                data: vec!["test1".to_string(), "test2".to_string()],
                nsp: "/".to_string(),
            },
            actual.1
        );
        assert_eq!(
            Opts {
                rooms: vec![],
                flags: Default::default()
            },
            actual.2
        );
    }

    #[test]
    fn emit_in_namespaces() {
        create_redis!(redis);
        let mut con = redis.get_connection().unwrap();
        let mut pubsub = con.as_pubsub();
        pubsub.subscribe("socket.io#/custom#").unwrap();

        // act
        let io = Emitter::new(redis);
        io.of("/custom").emit(vec!["test"]);

        // assert
        let actual = decode_msg(pubsub.get_message().unwrap());
        assert_eq!("emitter", actual.0);
        assert_eq!(
            Packet {
                _type: 2,
                data: vec!["test".to_string()],
                nsp: "/custom".to_string(),
            },
            actual.1
        );
        assert_eq!(
            Opts {
                rooms: vec![],
                flags: Default::default()
            },
            actual.2
        );
    }

    #[test]
    fn emit_to_namespaces() {
        create_redis!(redis);
        let mut con = redis.get_connection().unwrap();
        let mut pubsub = con.as_pubsub();
        pubsub.subscribe("socket.io#/custom#").unwrap();

        // act
        let io = Emitter::new(redis);
        io.of("/custom").emit(vec!["test"]);

        // assert
        let actual = decode_msg(pubsub.get_message().unwrap());
        assert_eq!("emitter", actual.0);
        assert_eq!(
            Packet {
                _type: 2,
                data: vec!["test".to_string()],
                nsp: "/custom".to_string(),
            },
            actual.1
        );
        assert_eq!(
            Opts {
                rooms: vec![],
                flags: Default::default()
            },
            actual.2
        );
    }

    #[test]
    fn emit_to_room() {
        create_redis!(redis);
        let mut con = redis.get_connection().unwrap();
        let mut pubsub = con.as_pubsub();
        pubsub.subscribe("socket.io#/#room1#").unwrap();

        // act
        let io = Emitter::new(redis);
        io.to("room1").emit(vec!["test"]);

        // assert
        let actual = decode_msg(pubsub.get_message().unwrap());
        assert_eq!("emitter", actual.0);
        assert_eq!(
            Packet {
                _type: 2,
                data: vec!["test".to_string()],
                nsp: "/".to_string(),
            },
            actual.1
        );
        assert_eq!(
            Opts {
                rooms: vec!["room1".to_string()],
                flags: Default::default()
            },
            actual.2
        );
    }
}
