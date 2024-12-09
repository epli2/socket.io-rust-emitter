//! A specific protocol used by the redis adapter.
//! 
//! Unfortunately, the line protocol differs between the Python and JS implementations.
//! This module provides a way to abstract over the differences.
#[cfg(feature = "python-v4")]
mod python_socketio;
#[cfg(feature = "js-v7")]
mod javascript;
