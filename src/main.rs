use libc::{c_char, mkfifo};
use serde::{Deserialize, Serialize};
#[cfg(target_family = "unix")]
use std::os::unix::ffi::OsStringExt;
use std::{
    env::args,
    fs, io,
    io::{Read, Write},
    mem, path, process, thread,
};

fn main() -> io::Result<()> {
    let mut args = args();
    // skip program name
    let _ = args.next();
    match args.next().as_ref().map(String::as_str) {
        Some("listen") => listen()?,
        Some("send") => {
            let msg = args.next().unwrap();
            send(msg)?;
        }
        _ => eprintln!("Please either listen or send"),
    }
    Ok(())
}

#[derive(Debug)]
pub struct Fifo {
    path: path::PathBuf,
}

impl Fifo {
    pub fn new(path: path::PathBuf) -> io::Result<Self> {
        let os_str = path.clone().into_os_string();
        let mut bytes = os_str.into_vec();
        bytes.push(0);

        let _ = fs::remove_file(&path);

        if unsafe { mkfifo((&bytes[0]) as *const u8 as *const c_char, 0o644) } != 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Fifo { path })
        }
    }

    pub fn open(&self) -> io::Result<FifoHandle> {
        let mut pipe = fs::OpenOptions::new().read(true).open(&self.path)?;
        let mut pid_bytes = [0u8; 4];
        pipe.read_exact(&mut pid_bytes)?;
        let pid = u32::from_ne_bytes(pid_bytes);

        println!("recv process id: {}", pid);

        drop(pipe);

        let read_fifo_path = format!("/tmp/rust-fifo-read.{}", pid);
        let read = fs::OpenOptions::new().read(true).open(&read_fifo_path)?;

        let write_fifo_path = format!("/tmp/rust-fifo-write.{}", pid);
        let write = fs::OpenOptions::new().write(true).open(&write_fifo_path)?;

        Ok(FifoHandle { read, write })
    }
}

impl Drop for Fifo {
    fn drop(&mut self) {
        println!(
            "Drop FIFO and remove pipe file: {}",
            &self.path.to_str().unwrap()
        );
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Serialize, Deserialize)]
pub enum Message {
    Print(String),
    Ack(),
}

pub struct FifoHandle {
    read: fs::File,
    write: fs::File,
}

impl FifoHandle {
    pub fn open<P: AsRef<path::Path>>(path: P) -> io::Result<Self> {
        let pid = process::id();

        println!("send process id: {}", pid);

        let read_fifo_path = format!("/tmp/rust-fifo-write.{}", pid);
        let read_fifo = Fifo::new(read_fifo_path.into())?;

        let write_fifo_path = format!("/tmp/rust-fifo-read.{}", pid);
        let write_fifo = Fifo::new(write_fifo_path.into())?;

        let mut pipe = fs::OpenOptions::new().write(true).open(path.as_ref())?;

        let pid_bytes: [u8; 4] = u32::to_ne_bytes(pid);
        pipe.write_all(&pid_bytes)?;
        pipe.flush()?;

        let write = fs::OpenOptions::new().write(true).open(&write_fifo.path)?;
        let read = fs::OpenOptions::new().read(true).open(&read_fifo.path)?;

        Ok(Self { read, write })
    }

    pub fn send_message(&mut self, msg: &Message) -> io::Result<()> {
        let msg = bincode::serialize(msg).expect("Serialization failed");
        self.write.write_all(&usize::to_ne_bytes(msg.len()))?;
        self.write.write_all(&msg[..])?;
        self.write.flush()
    }

    pub fn recv_message(&mut self) -> io::Result<Message> {
        let mut len_bytes = [0u8; mem::size_of::<usize>()];
        self.read.read_exact(&mut len_bytes)?;
        let len = usize::from_ne_bytes(len_bytes);

        let mut buf = vec![0; len];
        self.read.read_exact(&mut buf[..])?;

        Ok(bincode::deserialize(&buf[..]).expect("Deserialization failed"))
    }
}

fn listen() -> io::Result<()> {
    let fifo = Fifo::new(path::PathBuf::from("/tmp/rust-fifo"))?;
    loop {
        let mut handle = fifo.open()?;
        thread::spawn(move || {
            match handle.recv_message().expect("Failed to receive message") {
                Message::Print(p) => println!("{}", p),
                Message::Ack() => panic!("Didn't expect Ack now."),
            }
            #[allow(deprecated)]
            thread::sleep_ms(100);
            handle
                .send_message(&Message::Ack())
                .expect("Send message failed");
        });
    }
}

fn send(s: String) -> io::Result<()> {
    let mut handle = FifoHandle::open("/tmp/rust-fifo")?;
    #[allow(deprecated)]
    thread::sleep_ms(100);
    handle.send_message(&Message::Print(s))?;
    match handle.recv_message()? {
        Message::Print(p) => println!("{}", p),
        Message::Ack() => {}
    }
    Ok(())
}
