//! Simple TCP server for testing Task tools
//!
//! This server listens on port 8080 and echoes back any messages received.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(mut stream: TcpStream) {
    let peer = stream.peer_addr().unwrap();
    println!("[INFO] Client connected: {}", peer);

    let reader = BufReader::new(stream.try_clone().unwrap());

    for line in reader.lines() {
        match line {
            Ok(msg) => {
                println!("[RECV] {}: {}", peer, msg);

                if msg.trim() == "quit" {
                    println!("[INFO] Client {} requested disconnect", peer);
                    break;
                }

                let response = format!("ECHO: {}\n", msg);
                if let Err(e) = stream.write_all(response.as_bytes()) {
                    eprintln!("[ERROR] Failed to send response: {}", e);
                    break;
                }
            }
            Err(e) => {
                eprintln!("[ERROR] Read error from {}: {}", peer, e);
                break;
            }
        }
    }

    println!("[INFO] Client disconnected: {}", peer);
}

fn main() {
    let addr = "127.0.0.1:8080";

    println!("=================================");
    println!("  Test Server Starting...");
    println!("=================================");

    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[ERROR] Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    println!("[INFO] Server ready!");
    println!("[INFO] Listening on {}", addr);
    println!("=================================");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    handle_client(stream);
                });
            }
            Err(e) => {
                eprintln!("[ERROR] Connection failed: {}", e);
            }
        }
    }
}
