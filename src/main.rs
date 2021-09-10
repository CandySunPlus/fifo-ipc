use nix::{sys::stat, unistd};
use serde::{Deserialize, Serialize};
use std::{
    env::args,
    fs, io,
    io::{Read, Write},
    mem, path, thread,
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
        unistd::mkfifo(
            &path,
            stat::Mode::S_IRUSR | stat::Mode::S_IWUSR | stat::Mode::S_IRGRP | stat::Mode::S_IWGRP,
        )?;
        Ok(Fifo { path })
    }

    pub fn open(&self) -> io::Result<FifoHandle> {
        let pipe = fs::OpenOptions::new().read(true).open(&self.path)?;
        Ok(FifoHandle { pipe })
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
    pipe: fs::File,
}

impl FifoHandle {
    pub fn open<P: AsRef<path::Path>>(path: P) -> io::Result<Self> {
        let pipe = fs::OpenOptions::new().write(true).open(path.as_ref())?;
        Ok(Self { pipe })
    }

    pub fn send_message(&mut self, msg: &Message) -> io::Result<()> {
        let msg = bincode::serialize(msg).expect("Serialization failed");
        self.pipe.write_all(&usize::to_ne_bytes(msg.len()))?;
        self.pipe.write_all(&msg[..])?;
        self.pipe.flush()
    }

    pub fn recv_message(&mut self) -> io::Result<Message> {
        let mut len_bytes = [0u8; mem::size_of::<usize>()];
        self.pipe.read_exact(&mut len_bytes)?;
        let len = usize::from_ne_bytes(len_bytes);

        let mut buf = vec![0; len];
        self.pipe.read_exact(&mut buf[..])?;

        Ok(bincode::deserialize(&buf[..]).expect("Deserialization failed"))
    }
}

fn listen() -> io::Result<()> {
    let fifo = Fifo::new(path::PathBuf::from("/tmp/rust-fifo"))?;
    loop {
        println!("{}", "is block");
        let mut handle = fifo.open()?;
        // thread::spawn(move || {
        match handle.recv_message().expect("Failed to receive message") {
            Message::Print(p) => println!("{}", p),
            Message::Ack() => panic!("Didn't expect Ack now."),
        }
    }
}

fn send(s: String) -> io::Result<()> {
    let mut handle = FifoHandle::open("/tmp/rust-fifo")?;
    #[allow(deprecated)]
    thread::sleep_ms(100);
    handle.send_message(&Message::Print(s))?;
    Ok(())
}
