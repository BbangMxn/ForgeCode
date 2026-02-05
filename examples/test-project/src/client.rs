//! Simple TCP client for testing Task tools
//!
//! Connects to the server and sends test messages.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

fn main() {
    let addr = "127.0.0.1:8080";

    println!("=================================");
    println!("  Test Client Starting...");
    println!("=================================");

    let mut stream = match TcpStream::connect(addr) {
        Ok(s) => {
            println!("[INFO] Connected to {}", addr);
            s
        }
        Err(e) => {
            eprintln!("[ERROR] Failed to connect to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    let test_messages = vec!["Hello", "World", "Test message 1", "Test message 2", "quit"];

    let mut reader = BufReader::new(stream.try_clone().unwrap());

    for msg in test_messages {
        println!("[SEND] {}", msg);

        if let Err(e) = writeln!(stream, "{}", msg) {
            eprintln!("[ERROR] Failed to send: {}", e);
            break;
        }

        if msg == "quit" {
            break;
        }

        let mut response = String::new();
        match reader.read_line(&mut response) {
            Ok(_) => {
                println!("[RECV] {}", response.trim());
            }
            Err(e) => {
                eprintln!("[ERROR] Failed to receive: {}", e);
                break;
            }
        }
    }

    println!("=================================");
    println!("[INFO] Client finished");
    println!("=================================");
}
