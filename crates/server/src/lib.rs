mod config;
mod execution;
mod frontend;
pub mod installation;
pub mod paths;
pub mod service;
mod skills;
pub mod storage;

pub type Result<T> = std::result::Result<T, remote_codex_protocol::Fault>;

pub(crate) trait Checked<T> {
    fn checked(self, code: &str, message: &str) -> Result<T>;
}

impl<T, E> Checked<T> for std::result::Result<T, E> {
    fn checked(self, code: &str, message: &str) -> Result<T> {
        self.map_err(|_| remote_codex_protocol::Fault::new(code, message))
    }
}

mod profiles;
