//! Keeps the screen showing the current frames. Sleeps until the next frame
//! is due, so a still picture costs no CPU between sends.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::Error;
use crate::device::{Lcd, Transport};
use crate::frames::Frame;

const RETRY_AFTER: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub enum Command {
    /// Show these frames. `keep_alive` resends a still picture on an interval.
    Show {
        frames: Vec<Frame>,
        keep_alive: Option<Duration>,
    },
    Clear,
    Pause,
    Resume,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Idle,
    Showing,
    Paused,
    Disconnected(String),
}

pub fn run<T, C, S>(mut connect: C, commands: &Receiver<Command>, mut on_status: S)
where
    T: Transport,
    C: FnMut() -> Result<Lcd<T>, Error>,
    S: FnMut(&Status),
{
    let mut frames: Vec<Frame> = Vec::new();
    let mut keep_alive = None;
    let mut index = 0;
    let mut paused = false;
    let mut lcd: Option<Lcd<T>> = None;
    let mut due: Option<Instant> = None;
    let mut status = Status::Idle;
    let mut set_status = |s: Status, on_status: &mut S| {
        if s != status {
            status = s;
            on_status(&status);
        }
    };

    loop {
        let cmd = match due {
            Some(at) => match commands.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match commands.recv() {
                Ok(c) => Some(c),
                Err(_) => return,
            },
        };

        match cmd {
            Some(Command::Quit) => return,
            Some(Command::Show {
                frames: f,
                keep_alive: k,
            }) => {
                frames = f;
                keep_alive = k;
                index = 0;
                due = (!paused).then(Instant::now);
                continue;
            }
            Some(Command::Clear) => {
                frames.clear();
                due = None;
                set_status(Status::Idle, &mut on_status);
                continue;
            }
            Some(Command::Pause) => {
                paused = true;
                due = None;
                set_status(Status::Paused, &mut on_status);
                continue;
            }
            Some(Command::Resume) => {
                paused = false;
                due = Some(Instant::now());
                continue;
            }
            None => {}
        }

        if frames.is_empty() {
            due = None;
            set_status(Status::Idle, &mut on_status);
            continue;
        }

        if lcd.is_none() {
            match connect() {
                Ok(l) => lcd = Some(l),
                Err(e) => {
                    set_status(Status::Disconnected(e.to_string()), &mut on_status);
                    due = Some(Instant::now() + RETRY_AFTER);
                    continue;
                }
            }
        }

        let frame = &frames[index];
        let sent = lcd.as_mut().map_or(Ok(()), |l| l.send_jpeg(&frame.jpeg));
        match sent {
            Ok(()) => {
                set_status(Status::Showing, &mut on_status);
                due = if frames.len() > 1 {
                    index = (index + 1) % frames.len();
                    Some(Instant::now() + frame.delay)
                } else {
                    keep_alive.map(|k| Instant::now() + k)
                };
            }
            Err(e) => {
                lcd = None;
                set_status(Status::Disconnected(e.to_string()), &mut on_status);
                due = Some(Instant::now() + RETRY_AFTER);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{self, PACKET_LEN};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[derive(Clone, Default)]
    struct Shared {
        frames: Arc<Mutex<Vec<u8>>>,
        fail_writes: Arc<Mutex<bool>>,
    }

    struct Fake(Shared);

    impl Transport for Fake {
        fn write(&mut self, data: &[u8]) -> Result<(), Error> {
            if *self.0.fail_writes.lock().unwrap() {
                return Err(Error::NotFound);
            }
            // The header's byte 4 is the command; a JPEG frame header marks one sent frame.
            if data.len() >= PACKET_LEN && data[..4] == protocol::MAGIC && data[4] == 2 {
                self.0.frames.lock().unwrap().push(data[PACKET_LEN + 2]);
            }
            Ok(())
        }
        fn read(&mut self, _: usize) -> Result<Vec<u8>, Error> {
            let mut r = vec![0u8; PACKET_LEN];
            r[..4].copy_from_slice(&protocol::MAGIC);
            r[24] = 3;
            Ok(r)
        }
    }

    fn frame(tag: u8, delay_ms: u64) -> Frame {
        Frame {
            jpeg: vec![0xFF, 0xD8, tag, 0],
            delay: Duration::from_millis(delay_ms),
        }
    }

    fn start(shared: &Shared) -> (mpsc::Sender<Command>, thread::JoinHandle<Vec<Status>>) {
        let (tx, rx) = mpsc::channel();
        let s = shared.clone();
        let h = thread::spawn(move || {
            let mut seen = Vec::new();
            run(
                || Lcd::connect(Fake(s.clone())),
                &rx,
                |st| seen.push(st.clone()),
            );
            seen
        });
        (tx, h)
    }

    #[test]
    fn still_picture_is_sent_once() {
        let shared = Shared::default();
        let (tx, h) = start(&shared);
        tx.send(Command::Show {
            frames: vec![frame(7, 0)],
            keep_alive: None,
        })
        .unwrap();
        thread::sleep(Duration::from_millis(150));
        tx.send(Command::Quit).unwrap();
        let seen = h.join().unwrap();
        assert_eq!(*shared.frames.lock().unwrap(), vec![7]);
        assert_eq!(seen, vec![Status::Showing]);
    }

    #[test]
    fn animation_loops_in_order() {
        let shared = Shared::default();
        let (tx, h) = start(&shared);
        let frames = vec![frame(1, 20), frame(2, 20), frame(3, 20)];
        tx.send(Command::Show {
            frames,
            keep_alive: None,
        })
        .unwrap();
        thread::sleep(Duration::from_millis(200));
        tx.send(Command::Quit).unwrap();
        h.join().unwrap();
        let sent = shared.frames.lock().unwrap().clone();
        assert!(sent.len() >= 4, "{sent:?}");
        assert!(sent.windows(2).all(|w| w[1] == w[0] % 3 + 1), "{sent:?}");
    }

    #[test]
    fn keep_alive_resends() {
        let shared = Shared::default();
        let (tx, h) = start(&shared);
        let keep_alive = Some(Duration::from_millis(30));
        tx.send(Command::Show {
            frames: vec![frame(5, 0)],
            keep_alive,
        })
        .unwrap();
        thread::sleep(Duration::from_millis(200));
        tx.send(Command::Quit).unwrap();
        h.join().unwrap();
        assert!(shared.frames.lock().unwrap().len() >= 3);
    }

    #[test]
    fn pause_stops_sending() {
        let shared = Shared::default();
        let (tx, h) = start(&shared);
        tx.send(Command::Show {
            frames: vec![frame(1, 20), frame(2, 20)],
            keep_alive: None,
        })
        .unwrap();
        thread::sleep(Duration::from_millis(80));
        tx.send(Command::Pause).unwrap();
        thread::sleep(Duration::from_millis(30));
        let at_pause = shared.frames.lock().unwrap().len();
        thread::sleep(Duration::from_millis(120));
        assert_eq!(shared.frames.lock().unwrap().len(), at_pause);
        tx.send(Command::Quit).unwrap();
        assert!(h.join().unwrap().contains(&Status::Paused));
    }

    #[test]
    fn write_failure_reports_disconnected() {
        let shared = Shared::default();
        *shared.fail_writes.lock().unwrap() = true;
        let (tx, h) = start(&shared);
        tx.send(Command::Show {
            frames: vec![frame(1, 0)],
            keep_alive: None,
        })
        .unwrap();
        thread::sleep(Duration::from_millis(100));
        tx.send(Command::Quit).unwrap();
        let seen = h.join().unwrap();
        assert!(matches!(seen.first(), Some(Status::Disconnected(_))));
    }
}
