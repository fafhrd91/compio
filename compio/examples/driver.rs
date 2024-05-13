use compio::{
    buf::{arrayvec::ArrayVec, IntoInner},
    driver::{
        op::{CloseFile, ReadAt},
        AsRawFd, OpCode, OwnedFd, Proactor, PushEntry, SharedFd,
    },
};

#[cfg(windows)]
fn open_file(driver: &mut Proactor) -> OwnedFd {
    use std::os::windows::{
        fs::OpenOptionsExt,
        io::{FromRawHandle, IntoRawHandle, OwnedHandle},
    };

    use compio::{driver::op::Asyncify, BufResult};
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OVERLAPPED;

    let op = Asyncify::new(|| {
        BufResult(
            std::fs::OpenOptions::new()
                .read(true)
                .attributes(FILE_FLAG_OVERLAPPED)
                .open("Cargo.toml")
                .map(|f| f.into_raw_handle() as usize),
            (),
        )
    });
    let (fd, _) = push_and_wait(driver, op);
    OwnedFd::File(unsafe { OwnedHandle::from_raw_handle(fd as _) })
}

#[cfg(unix)]
fn open_file(driver: &mut Proactor) -> OwnedFd {
    use std::{ffi::CString, os::fd::FromRawFd};

    use compio_driver::op::OpenFile;

    let op = OpenFile::new(
        CString::new("Cargo.toml").unwrap(),
        libc::O_CLOEXEC | libc::O_RDONLY,
        0o666,
    );
    let (fd, _) = push_and_wait(driver, op);
    unsafe { OwnedFd::from_raw_fd(fd as _) }
}

fn push_and_wait<O: OpCode + 'static>(driver: &mut Proactor, op: O) -> (usize, O) {
    match driver.push(op) {
        PushEntry::Ready(res) => res.unwrap(),
        PushEntry::Pending(user_data) => {
            let mut entries = ArrayVec::<usize, 1>::new();
            while entries.is_empty() {
                driver.poll(None, &mut entries).unwrap();
            }
            assert_eq!(entries[0], *user_data);
            driver.pop(user_data).unwrap().unwrap()
        }
    }
}

fn main() {
    let mut driver = Proactor::new().unwrap();

    let fd = open_file(&mut driver);
    let fd = SharedFd::new(fd);
    driver.attach(fd.as_raw_fd()).unwrap();

    let op = ReadAt::new(fd.clone(), 0, Vec::with_capacity(4096));
    let (n, op) = push_and_wait(&mut driver, op);

    let mut buffer = op.into_inner();
    unsafe {
        buffer.set_len(n);
    }
    println!("{}", String::from_utf8(buffer).unwrap());

    let op = CloseFile::new(fd.try_unwrap().unwrap());
    push_and_wait(&mut driver, op);
}
