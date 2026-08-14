use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::event::{BotEvent, encode_event};

#[derive(Debug, Default)]
struct IpcCounters {
    sequence: AtomicU64,
    queued: AtomicU64,
    delivered: AtomicU64,
    dropped_queue_full: AtomicU64,
    dropped_without_consumer: AtomicU64,
    consumer_disconnects: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcStats {
    pub last_sequence: u64,
    pub queued: u64,
    pub delivered: u64,
    pub dropped_queue_full: u64,
    pub dropped_without_consumer: u64,
    pub consumer_disconnects: u64,
}

#[derive(Clone)]
pub struct EventSink {
    sender: SyncSender<BotEvent>,
    counters: Arc<IpcCounters>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    Queued { sequence: u64 },
    DroppedQueueFull { sequence: u64 },
    PublisherStopped { sequence: u64 },
}

impl EventSink {
    pub fn publish(&self, mut event: BotEvent) -> PublishOutcome {
        let sequence = self
            .counters
            .sequence
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        event.sequence = sequence;

        match self.sender.try_send(event) {
            Ok(()) => {
                self.counters.queued.fetch_add(1, Ordering::Relaxed);
                PublishOutcome::Queued { sequence }
            }
            Err(TrySendError::Full(_)) => {
                self.counters
                    .dropped_queue_full
                    .fetch_add(1, Ordering::Relaxed);
                PublishOutcome::DroppedQueueFull { sequence }
            }
            Err(TrySendError::Disconnected(_)) => PublishOutcome::PublisherStopped { sequence },
        }
    }

    #[must_use]
    pub fn stats(&self) -> IpcStats {
        IpcStats {
            last_sequence: self.counters.sequence.load(Ordering::Relaxed),
            queued: self.counters.queued.load(Ordering::Relaxed),
            delivered: self.counters.delivered.load(Ordering::Relaxed),
            dropped_queue_full: self.counters.dropped_queue_full.load(Ordering::Relaxed),
            dropped_without_consumer: self
                .counters
                .dropped_without_consumer
                .load(Ordering::Relaxed),
            consumer_disconnects: self.counters.consumer_disconnects.load(Ordering::Relaxed),
        }
    }
}

pub struct IpcPublisher {
    sink: EventSink,
    _handle: JoinHandle<()>,
}

impl IpcPublisher {
    pub fn bind(bind_address: &str, queue_capacity: usize) -> io::Result<Self> {
        let socket_address = bind_address.parse::<SocketAddr>().map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid IPC bind address {bind_address}: {error}"),
            )
        })?;
        if !socket_address.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "bot IPC must bind to a loopback address",
            ));
        }

        let listener = TcpListener::bind(socket_address)?;
        listener.set_nonblocking(true)?;
        let (sender, receiver) = mpsc::sync_channel(queue_capacity.max(1));
        let counters = Arc::new(IpcCounters::default());
        let worker_counters = Arc::clone(&counters);
        let handle = thread::Builder::new()
            .name("alephium-bot-ipc".to_owned())
            .spawn(move || run_publisher(listener, receiver, worker_counters))?;

        Ok(Self {
            sink: EventSink { sender, counters },
            _handle: handle,
        })
    }

    #[must_use]
    pub fn sink(&self) -> EventSink {
        self.sink.clone()
    }
}

fn run_publisher(listener: TcpListener, receiver: Receiver<BotEvent>, counters: Arc<IpcCounters>) {
    let mut consumer: Option<TcpStream> = None;
    loop {
        if consumer.is_none() {
            consumer = accept_consumer(&listener);
        }

        let event = match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };

        let Some(stream) = consumer.as_mut() else {
            counters
                .dropped_without_consumer
                .fetch_add(1, Ordering::Relaxed);
            continue;
        };

        let frame = match encode_event(&event) {
            Ok(frame) => frame,
            Err(_) => {
                counters
                    .dropped_without_consumer
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };

        if stream.write_all(&frame).is_ok() {
            counters.delivered.fetch_add(1, Ordering::Relaxed);
        } else {
            counters
                .consumer_disconnects
                .fetch_add(1, Ordering::Relaxed);
            consumer = None;
        }
    }
}

fn accept_consumer(listener: &TcpListener) -> Option<TcpStream> {
    match listener.accept() {
        Ok((stream, address)) if address.ip().is_loopback() => {
            let _ = stream.set_nodelay(true);
            let _ = stream.set_write_timeout(Some(Duration::from_millis(50)));
            Some(stream)
        }
        Ok((_stream, _address)) => None,
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => None,
        Err(_) => None,
    }
}
