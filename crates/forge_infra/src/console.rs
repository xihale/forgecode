//! Shared output printer for synchronized writes to stdout/stderr.
//!
//! Prevents interleaving when multiple threads write to terminal output.
//!
//! Broken pipe errors (EPIPE / `os error 32`) are silently suppressed. When
//! output is piped to another process (e.g. `forge | head`) and the consumer
//! closes early, further writes would fail with `BrokenPipe`. Rather than
//! propagating this as a fatal error, we treat it as a normal termination
//! signal and pretend the write succeeded.

use std::io::{self, ErrorKind, Stderr, Stdout, Write};
use std::sync::Arc;

use forge_domain::ConsoleWriter;
use std::sync::Mutex;

/// Returns `Ok(buf.len())` if the error is a broken pipe, otherwise `Err`.
///
/// When stdout is piped to a consumer that closes early (e.g. `head`, `jq`),
/// subsequent writes return `ErrorKind::BrokenPipe`. This is expected — the
/// consumer has finished reading — so we swallow the error and report success
/// to callers so they don't crash or display spurious error messages.
fn suppress_broken_pipe(res: io::Result<usize>, buf_len: usize) -> io::Result<usize> {
    match res {
        Ok(n) => Ok(n),
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(buf_len),
        Err(e) => Err(e),
    }
}

/// Returns `Ok(())` if the error is a broken pipe, otherwise `Err`.
fn suppress_broken_pipe_flush(res: io::Result<()>) -> io::Result<()> {
    match res {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(e),
    }
}

/// Thread-safe output printer that synchronizes writes to stdout/stderr.
///
/// Wraps writers in mutexes to prevent output interleaving when multiple
/// threads (e.g., streaming markdown and shell commands) write concurrently.
///
/// Generic over writer types `O` (stdout) and `E` (stderr) to support testing
/// with mock writers.
#[derive(Debug)]
pub struct StdConsoleWriter<O = Stdout, E = Stderr> {
    stdout: Arc<Mutex<O>>,
    stderr: Arc<Mutex<E>>,
}

impl<O, E> Clone for StdConsoleWriter<O, E> {
    fn clone(&self) -> Self {
        Self { stdout: self.stdout.clone(), stderr: self.stderr.clone() }
    }
}

impl Default for StdConsoleWriter<Stdout, Stderr> {
    fn default() -> Self {
        Self {
            stdout: Arc::new(Mutex::new(io::stdout())),
            stderr: Arc::new(Mutex::new(io::stderr())),
        }
    }
}

impl<O, E> StdConsoleWriter<O, E> {
    /// Creates a new OutputPrinter with custom writers.
    pub fn with_writers(stdout: O, stderr: E) -> Self {
        Self {
            stdout: Arc::new(Mutex::new(stdout)),
            stderr: Arc::new(Mutex::new(stderr)),
        }
    }
}

impl<O: Write + Send, E: Write + Send> ConsoleWriter for StdConsoleWriter<O, E> {
    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self.stdout.lock().unwrap_or_else(|e| e.into_inner());
        suppress_broken_pipe(guard.write(buf), buf.len())
    }

    fn write_err(&self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self.stderr.lock().unwrap_or_else(|e| e.into_inner());
        suppress_broken_pipe(guard.write(buf), buf.len())
    }

    fn flush(&self) -> io::Result<()> {
        let mut guard = self.stdout.lock().unwrap_or_else(|e| e.into_inner());
        suppress_broken_pipe_flush(guard.flush())
    }

    fn flush_err(&self) -> io::Result<()> {
        let mut guard = self.stderr.lock().unwrap_or_else(|e| e.into_inner());
        suppress_broken_pipe_flush(guard.flush())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::thread;

    use bstr::ByteSlice;

    use super::*;

    #[test]
    fn test_concurrent_writes_dont_interleave() {
        let stdout = Cursor::new(Vec::new());
        let stderr = Cursor::new(Vec::new());
        let printer = StdConsoleWriter::with_writers(stdout, stderr);
        let p1 = printer.clone();
        let p2 = printer.clone();

        let h1 = thread::spawn(move || {
            p1.write(b"AAAA").unwrap();
            p1.write(b"BBBB").unwrap();
            p1.flush().unwrap();
        });

        let h2 = thread::spawn(move || {
            p2.write(b"XXXX").unwrap();
            p2.write(b"ZZZZ").unwrap();
            p2.flush().unwrap();
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // Verify output is one of the valid orderings where individual writes are
        // atomic but sequences can interleave. AAAA must come before BBBB, XXXX
        // must come before ZZZZ
        let actual = printer.stdout.lock().unwrap().get_ref().clone();
        let valid_orderings = [
            b"AAAABBBBXXXXZZZZ".to_vec(), // Thread 1 completes, then Thread 2
            b"XXXXZZZZAAAABBBB".to_vec(), // Thread 2 completes, then Thread 1
            b"AAAAXXXXBBBBZZZZ".to_vec(), // A, X, B, Z
            b"AAAAXXXXZZZZBBBB".to_vec(), // A, X, Z, B
            b"XXXXAAAABBBBZZZZ".to_vec(), // X, A, B, Z
            b"XXXXAAAAZZZZBBBB".to_vec(), // X, A, Z, B
        ];
        assert!(
            valid_orderings.contains(&actual),
            "Output was interleaved: {:?}",
            actual.as_slice().to_str_lossy()
        );
    }

    #[test]
    fn test_with_mock_writer() {
        let stdout = Cursor::new(Vec::new());
        let stderr = Cursor::new(Vec::new());
        let printer = StdConsoleWriter::with_writers(stdout, stderr);

        printer.write(b"hello").unwrap();
        printer.write_err(b"error").unwrap();

        let stdout_content = printer.stdout.lock().unwrap().get_ref().clone();
        let stderr_content = printer.stderr.lock().unwrap().get_ref().clone();

        assert_eq!(stdout_content, b"hello");
        assert_eq!(stderr_content, b"error");
    }

    /// A writer that always returns `BrokenPipe` to simulate a closed consumer.
    #[derive(Debug)]
    struct BrokenPipeWriter;

    impl Write for BrokenPipeWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(ErrorKind::BrokenPipe, "Broken pipe (os error 32)"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(ErrorKind::BrokenPipe, "Broken pipe (os error 32)"))
        }
    }

    #[test]
    fn test_broken_pipe_is_suppressed_on_write() {
        let printer = StdConsoleWriter::with_writers(BrokenPipeWriter, BrokenPipeWriter);

        // Write should succeed despite underlying writer returning BrokenPipe
        let actual = printer.write(b"hello").unwrap();
        let expected = 5;
        assert_eq!(actual, expected);

        let actual = printer.write_err(b"error").unwrap();
        let expected = 5;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_broken_pipe_is_suppressed_on_flush() {
        let printer = StdConsoleWriter::with_writers(BrokenPipeWriter, BrokenPipeWriter);

        // Flush should succeed despite underlying writer returning BrokenPipe
        printer.flush().unwrap();
        printer.flush_err().unwrap();
    }

    #[test]
    fn test_non_broken_pipe_errors_propagate() {
        // Other error kinds should still propagate — test via suppress_broken_pipe directly
        let res: io::Result<usize> = Err(io::Error::new(ErrorKind::PermissionDenied, "denied"));
        let actual = suppress_broken_pipe(res, 5);
        assert!(actual.is_err());
        let err = actual.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::PermissionDenied);
    }
}
