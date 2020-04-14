use std::os::unix::io::RawFd;
use std::{
    path::{
        Path,
        PathBuf
    },
    process,
    format
};
use termios::{Termios, ICANON, VTIME, VMIN};

fn main() {
    let raw_stdin = 0;
    let mut termios = Termios::from_fd(raw_stdin).expect("Stdin is not a tty!");
    println!("Your terminal is: {:?}", get_tty_path());
    println!("Terminal settings: {:?}", termios);
    if termios.c_lflag & ICANON != 0 {
        println!("Terminal is canon!");
    }
    else {
        println!("Terminal is not canon");
    }
    println!("VTIME: {}", termios.c_cc[VTIME]);
    println!("VMIN: {}", termios.c_cc[VMIN]);
}

fn get_tty_path() -> PathBuf {
    let pid = process::id();
    let path = format!("/proc/{}/fd/0", pid);
    let path = Path::new(&path);
    path.canonicalize().expect("Invalid path")
}
