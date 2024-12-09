use std::collections::HashMap;

use crate::Emitter;
use redis::Commands;
use rmp_serde::Serializer;
use serde::Serialize;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Packet {
    #[serde(rename = "type")]
    _type: i32,
    data: Vec<String>,
    nsp: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Opts {
    rooms: Vec<String>,
    flags: HashMap<String, bool>,
}

impl Emitter {
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
    use super::{Emitter, Opts, Packet};
    use crate::tests::create_redis;
    use redis::Msg;
    use rmp_serde::Deserializer;
    use serde::Deserialize;

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
