#[cfg(feature = "js-v7")]
#[macro_use]
extern crate serde_derive;
#[cfg(feature = "js-v7")]
use std::collections::HashMap;

mod implementations;

#[derive(Debug, Clone)]
pub struct Emitter {
    redis: redis::Client,
    prefix: String,
    nsp: String,
    channel: String,
    rooms: Vec<String>,
    #[cfg(feature = "js-v7")]
    flags: HashMap<String, bool>,
    #[cfg(feature = "js-v7")]
    uid: String,
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
        #[cfg(feature = "js-v7")]
        channel: format!("{}#{}#", prefix, nsp),
        #[cfg(feature = "python-v4")]
        channel: "socketio".to_string(),
        rooms: Vec::new(),
        #[cfg(feature = "js-v7")]
        flags: HashMap::new(),
        #[cfg(feature = "js-v7")]
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

    // Emitting functions are added in the implementation modules.
}

#[cfg(all(not(feature = "js-v7"), not(feature = "python-v4")))]
compile_error!("At least one of the features 'js-v7' or 'python-v4' must be enabled.");

#[cfg(all(feature = "js-v7", feature = "python-v4"))]
compile_error!("Only one of the features 'js-v7' or 'python-v4' can be enabled.");

#[cfg(test)]
pub(crate) mod tests {
    pub const DOCKER_NETWORK_NAME: &str = "testcontainers-socketio";
    pub(crate) fn launch_redis_container() -> testcontainers::Container<testcontainers::GenericImage>
    {
        use testcontainers::runners::SyncRunner;
        let redis = testcontainers::GenericImage::new("redis", "latest")
            .with_exposed_port(testcontainers::core::ContainerPort::Tcp(6379))
            .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
                "Ready to accept connections",
            ))
            .with_network(DOCKER_NETWORK_NAME)
            .start()
            .unwrap();
        redis
    }

    #[cfg(feature = "js-v7")]
    macro_rules! create_redis {
        ($redis:ident) => {
            let redis = crate::tests::launch_redis_container();
            let redis_url = format!(
                "redis://localhost:{}",
                redis.get_host_port_ipv4(6379).unwrap()
            );
            let $redis = redis::Client::open(redis_url.as_str()).unwrap();
        };
    }

    #[cfg(feature = "js-v7")]
    pub(crate) use create_redis;
    use testcontainers::ImageExt;
}
