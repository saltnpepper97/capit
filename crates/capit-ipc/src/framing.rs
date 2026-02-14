// Author: Dustin Pilgrim
// License: MIT

use std::io::{Read, Write};

use crate::error::{IpcError, Result};

pub fn write_frame<W: Write>(mut w: W, bytes: &[u8]) -> Result<()> {
    let len: u32 = bytes
        .len()
        .try_into()
        .map_err(|_| IpcError::FrameTooLarge)?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(bytes)?;
    w.flush()?;
    Ok(())
}

pub fn read_frame<R: Read>(mut r: R, max_len: usize) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > max_len {
        return Err(IpcError::FrameTooLarge);
    }

    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}
