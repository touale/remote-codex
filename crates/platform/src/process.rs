//! Owned child processes and their descendants.
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use std::io;
use tokio::process::Command;

pub struct Child(Box<dyn ChildWrapper>);

impl Child {
    pub fn spawn(command: Command) -> io::Result<Self> {
        let mut command = CommandWrap::from(command);
        command.wrap(KillOnDrop);
        #[cfg(unix)]
        command.wrap(process_wrap::tokio::ProcessGroup::leader());
        #[cfg(windows)]
        {
            let mut flags = process_wrap::tokio::CreationFlags(Default::default());
            flags.0.0 = windows_sys::Win32::System::Threading::CREATE_NO_WINDOW; // CREATE_NO_WINDOW, preserved when JobObject adds CREATE_SUSPENDED.
            command.wrap(flags).wrap(process_wrap::tokio::JobObject);
        }
        command.spawn().map(Self)
    }
}

impl std::ops::Deref for Child {
    type Target = dyn ChildWrapper;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
impl std::ops::DerefMut for Child {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.0
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.start_kill();
    }
}

#[cfg(windows)]
pub fn hide_console(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
}
