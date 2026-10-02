// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::io::{Read, Write};
use std::sync::Mutex;

use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
};

use wxc_common::process_util::{create_std_pipes, SendOwnedHandle};
use wxc_common::sandbox_process::{NativeStdio, PtySize};

pub(crate) struct PseudoConsole {
    handle: HPCON,
    input: Mutex<Option<SendOwnedHandle>>,
    output: SendOwnedHandle,
    size: Mutex<PtySize>,
}

unsafe impl Send for PseudoConsole {}

impl PseudoConsole {
    pub(crate) fn new(size: PtySize) -> std::io::Result<Self> {
        let coord = coord(size)?;
        let (input_read, mut input_write) =
            create_std_pipes(false).map_err(std::io::Error::other)?;
        let (mut output_read, output_write) =
            create_std_pipes(true).map_err(std::io::Error::other)?;
        let handle = unsafe { CreatePseudoConsole(coord, input_read.get(), output_write.get(), 0) }
            .map_err(std::io::Error::other)?;

        Ok(Self {
            handle,
            input: Mutex::new(Some(SendOwnedHandle::take(&mut input_write))),
            output: SendOwnedHandle::take(&mut output_read),
            size: Mutex::new(size),
        })
    }

    pub(crate) fn attribute_value(&self) -> *const core::ffi::c_void {
        self.handle.0 as *const core::ffi::c_void
    }

    pub(crate) fn clone_reader(&self) -> std::io::Result<Box<dyn Read + Send>> {
        let handle = self.output.try_clone_owned_handle()?;
        Ok(Box::new(std::fs::File::from(handle)))
    }

    pub(crate) fn take_writer(&self) -> std::io::Result<Box<dyn Write + Send>> {
        let writer = self
            .input
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "the PTY writer has already been taken",
                )
            })?;
        Ok(Box::new(std::fs::File::from(
            writer.into_std_owned_handle()?,
        )))
    }

    pub(crate) fn take_native_stdio(&self) -> std::io::Result<NativeStdio> {
        let writer = self
            .input
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "the PTY writer has already been taken",
                )
            })?;
        Ok(NativeStdio {
            stdin: Some(writer.into_std_owned_handle()?),
            stdout: Some(self.output.try_clone_owned_handle()?),
            stderr: None,
        })
    }

    pub(crate) fn resize(&self, size: PtySize) -> std::io::Result<()> {
        unsafe { ResizePseudoConsole(self.handle, coord(size)?) }.map_err(std::io::Error::other)?;
        *self
            .size
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = size;
        Ok(())
    }

    pub(crate) fn size(&self) -> PtySize {
        *self
            .size
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Drop for PseudoConsole {
    fn drop(&mut self) {
        unsafe { ClosePseudoConsole(self.handle) };
    }
}

fn coord(size: PtySize) -> std::io::Result<COORD> {
    if size.cols == 0
        || size.rows == 0
        || size.cols > i16::MAX as u16
        || size.rows > i16::MAX as u16
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "PTY rows and columns must be between 1 and 32767",
        ));
    }
    Ok(COORD {
        X: size.cols as i16,
        Y: size.rows as i16,
    })
}
