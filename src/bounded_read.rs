use crate::ports::ReadControl;
use std::io::{self, Read};
pub(crate) struct ControlledRead<'a, R> {
    pub reader: R,
    pub control: Option<&'a ReadControl>,
}
impl<R: Read> Read for ControlledRead<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = if let Some(control) = self.control {
            control.check()?;
            buffer
                .len()
                .min(64 * 1024)
                .min(control.remaining().saturating_add(1).min(usize::MAX as u64) as usize)
        } else {
            buffer.len()
        };
        let read = self.reader.read(&mut buffer[..count])?;
        if let Some(control) = self.control {
            control.record_read(read)?;
        }
        Ok(read)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Cursor, time::Duration};
    #[test]
    fn cancellation_after_first_chunk_stops_the_next_read() {
        struct CancelAfterRead<'a>(&'a ReadControl);
        impl Read for CancelAfterRead<'_> {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                output.fill(0);
                self.0.cancel();
                Ok(output.len())
            }
        }
        let control = ReadControl::new(1_000_000, Duration::from_secs(1));
        let mut read = ControlledRead {
            reader: CancelAfterRead(&control),
            control: Some(&control),
        };
        let mut output = vec![0; 100_000];
        assert!(read.read_exact(&mut output).is_err());
        assert_eq!(control.bytes_read(), 64 * 1024);
    }
    #[test]
    fn cancellation_budget_and_deadline_apply_inside_reads() {
        let bytes = vec![0; 1_000_000];
        for (limit, cancel, duration) in [
            (1000, false, Duration::from_secs(1)),
            (1_000_000, true, Duration::from_secs(1)),
            (1_000_000, false, Duration::ZERO),
        ] {
            let control = ReadControl::new(limit, duration);
            if cancel {
                control.cancel();
            }
            let mut read = ControlledRead {
                reader: Cursor::new(&bytes),
                control: Some(&control),
            };
            let mut output = Vec::new();
            assert!(read.read_to_end(&mut output).is_err());
            assert!(control.bytes_read() <= limit + 1);
            if cancel || duration.is_zero() {
                assert_eq!(control.bytes_read(), 0);
            }
        }
    }
}
