//! Writes to a process (terminal input, a run's stdin) that never block the caller.
//!
//! A pipe or pseudo terminal accepts only a few kilobytes while the program does not read;
//! a blocking `write_all` from an IPC command would then freeze the whole window. The bytes
//! go into a queue instead, and a writer thread per process feeds them in order.

use std::io::Write;
use std::sync::mpsc;
use std::sync::Mutex;

/// The input side of a process, fed in order by its own writer thread.
pub(crate) struct QueuedWriter {
    tx: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
}

impl QueuedWriter {
    /// Start the writer thread for `writer`.
    pub(crate) fn spawn(
        name: String,
        mut writer: impl Write + Send + 'static,
    ) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new().name(name).spawn(move || {
            while let Ok(chunk) = rx.recv() {
                if writer
                    .write_all(&chunk)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    // The program is gone or closed its input: later writes fail.
                    break;
                }
            }
        })?;
        Ok(Self {
            tx: Mutex::new(Some(tx)),
        })
    }

    /// Queue `data`; fails only when the input is closed or an earlier write failed.
    pub(crate) fn write(&self, data: &[u8]) -> Result<(), &'static str> {
        let guard = self.tx.lock().expect("input queue poisoned");
        let tx = guard.as_ref().ok_or("input is closed")?;
        tx.send(data.to_vec()).map_err(|_| "input is closed")
    }

    /// Close the input once everything queued so far has been written (end of file).
    pub(crate) fn close(&self) {
        self.tx.lock().expect("input queue poisoned").take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Shared(Arc<Mutex<Vec<u8>>>);
    impl Write for Shared {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn writes_in_order_and_refuses_after_close() {
        let out = Arc::new(Mutex::new(Vec::new()));
        let w = QueuedWriter::spawn("t".into(), Shared(out.clone())).unwrap();
        w.write(b"ab").unwrap();
        w.write(b"cd").unwrap();
        w.close();
        assert!(w.write(b"x").is_err());
        for _ in 0..200 {
            if out.lock().unwrap().len() == 4 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(&*out.lock().unwrap(), b"abcd");
    }
}
