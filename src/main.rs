use std::io::prelude::*;
use std::fs;
use std::net::TcpListener;
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    let pool = ThreadPool::new(4);

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        pool.spawn(|| {
            handle_connection(stream);
        });
    }
}

// hack because box won't take ownerhisp or something...
// see: https://doc.rust-lang.org/book/second-edition/ch20-02-multithreaded.html
// near the end...
pub trait FnBox {
    fn call_box(self: Box<Self>);
}

impl<F: FnOnce()> FnBox for F {
    fn call_box(self: Box<F>) {
        (*self)()
    }
}

type Job = Box<FnBox + Send + 'static>;

struct Worker {
    id: usize,
    thread: thread::JoinHandle<Arc<Mutex<mpsc::Receiver<Job>>>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
        Worker {
            id,
            thread: thread::spawn(move || {
                loop {
                    //receiver.lock() gives exclusive access to the receiver channel.
                    //so the first thread to call this well be the next to run a job.
                    //Every other thread well sleep on lock. When it's unlocked from going out of
                    //scope (in a diff thread), im guessing the first function to call lock() after
                    //the first, would get next dibs. Meanwhile the thread that just finished well
                    //call lock, and place itsself in line.
                    //
                    //recv sleeps still it receives a message
                    let job = receiver.lock().unwrap().recv().unwrap();

                    println!("Worker {} got job; executing.", id);

                    job.call_box();
                }
            }),
        }
    }
}

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<Job>,
}

impl ThreadPool {
    fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let mut workers = Vec::with_capacity(size);
        let (sender, receiver) = mpsc::channel();

        // Because we want to share the receiver between threads.
        let receiver = Arc::new(Mutex::new(receiver));

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)))
        }

        ThreadPool {
            workers,
            sender,
        }
    }

    pub fn spawn<F>(&self, f: F)
        where F: FnBox + Send + 'static
    {
        let job = Box::new(f);

        self.sender.send(job).unwrap();
    }
}

fn handle_connection(mut stream: TcpStream) {
    let mut buffer = [0; 512];

    stream.read(&mut buffer).unwrap();
    println!("Request: {}", String::from_utf8_lossy(&buffer[..]));

	let get = b"GET / HTTP/1.1\r\n";
	let sleep = b"GET /sleep HTTP/1.1\r\n";

    let (status, html_file) = if buffer.starts_with(get) {
		("HTTP/1.1 200 OK\r\n\r\n", "hello.html")
	} else if buffer.starts_with(sleep) {
	    thread::sleep(Duration::from_secs(5));
		("HTTP/1.1 200 OK\r\n\r\n", "hello.html")
    } else {
		("HTTP/1.1 404 NOT FOUND\r\n\r\n", "404.html")
    };

    let response = format!("{}{}", status, fs::read_to_string(html_file).unwrap());

    stream.write(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}
