//! This program demonstrates how to implement a client for the GNU
//! job server. It tries to obtain a single token and gives it back
//! immediately.

use std::{
    env,
    fs::File,
    io::{Read, Write},
    os::fd::FromRawFd,
};

use anyhow::Context;
use thiserror::Error;

#[derive(Debug, PartialEq)]
enum JobServerStyle<'a> {
    #[cfg(target_family = "unix")]
    /// The Fifo job server style is supported since Make 4.4 and is a FIFO/named pipe.
    Fifo(&'a str),
    /// Pipe is the older implementation, supported since ages. It
    /// consists of two file descriptors, the first one is for reading
    /// the second one for writing.
    Pipe(i32, i32),
    #[cfg(target_os = "windows")]
    /// Sem is for Win32 semaphore
    Sem,
}

#[derive(Error, Debug, PartialEq)]
enum ParseJobserverAuthError {
    #[error("invalid jobserver auth \"{0}\"")]
    InvalidJobServerAuth(String),
    #[error("invalid pipe descriptors")]
    InvalidPipeDescriptors,
}

// parse jobserver auth
#[cfg(target_family = "unix")]
fn parse_jobserver_auth(makeflags: &str) -> Result<JobServerStyle<'_>, ParseJobserverAuthError> {
    // quick and dirty implementation, don't look too closely!

    if let Some(pos) = makeflags.rfind("--jobserver-auth=fifo:") {
        let pos_eq = pos + "--jobserver-auth=fifo:".as_bytes().len();
        if let Some(space_pos) = makeflags[pos_eq..].find(' ') {
            return Ok(JobServerStyle::Fifo(
                &makeflags[pos_eq..(pos_eq + space_pos)],
            ));
        } else {
            return Ok(JobServerStyle::Fifo(&makeflags[pos_eq..]));
        }
    }

    if let Some(pos) = makeflags.rfind("--jobserver-auth=") {
        let pos_eq = pos + "--jobserver-auth=".as_bytes().len();
        if makeflags[pos_eq..]
            .find(|c: char| c == '-' || c.is_ascii_digit())
            .is_some()
        {
            let splits: Vec<_> = if let Some(space_pos) = makeflags[pos_eq..].find(' ') {
                makeflags[pos_eq..(pos_eq + space_pos)].split(',').collect()
            } else {
                makeflags[pos_eq..].split(',').collect()
            };

            if splits.len() != 2 {
                return Err(ParseJobserverAuthError::InvalidPipeDescriptors);
            }

            return Ok(JobServerStyle::Pipe(
                splits[0].parse::<i32>().unwrap(),
                splits[1].parse::<i32>().unwrap(),
            ));
        }
    }

    Err(ParseJobserverAuthError::InvalidJobServerAuth(
        makeflags.to_string(),
    ))
}

#[cfg(target_os = "windows")]
fn parse_jobserver_auth(makeflags: &str) -> Result<JobServerStyle<'_>, ParseJobserverAuthError> {
    unimplemented!("windows semaphores");
}

fn main() -> anyhow::Result<()> {
    match env::var("MAKEFLAGS") {
        Ok(makeflags) => {
            if makeflags.contains("--jobserver-auth=") {
                println!("jobserver present {makeflags}");
            }
            let job_server_style =
                parse_jobserver_auth(&makeflags).context("parsing jobserver auth")?;

            match job_server_style {
                JobServerStyle::Fifo(fifo_file) => {
                    let mut fifo = File::options()
                        .read(true)
                        .write(true)
                        .create_new(false)
                        .open(fifo_file)?;
                    let mut token: [u8; 2] = [0; 2];
                    // try to get the token
                    fifo.read_exact(&mut token).context("acquiring token")?;
                    println!("{}", char::from(token[0]));
                    println!("{}", char::from(token[1]));
                    fifo.write_all(&token).context("releasing token")?;
                }
                JobServerStyle::Pipe(read_fd, write_fd) => {
                    if read_fd < 0 || write_fd < 0 {
                        eprintln!("warning: cannot use jobserver, because of negative pipe file descriptors");
                    } else {
                        let mut file_read = unsafe { File::from_raw_fd(read_fd) };
                        let mut file_write = unsafe { File::from_raw_fd(write_fd) };

                        let mut token: [u8; 1] = [0; 1];
                        file_read
                            .read_exact(&mut token)
                            .context("acquiring token")?;
                        println!("{}", char::from(token[0]));
                        file_write.write_all(&token).context("releasing token")?;
                    }
                }
            }
        }
        Err(_) => {
            eprintln!("warning: jobserver not available");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_jobserver_auth_fifo() {
        assert_eq!(
            super::parse_jobserver_auth(" -j2 --jobserver-auth=fifo:/tmp/GMfifo6851"),
            Ok(super::JobServerStyle::Fifo("/tmp/GMfifo6851")),
        );

        assert_eq!(
            super::parse_jobserver_auth(" -j2 --jobserver-auth=fifo:/tmp/GMfifo6851 -blah"),
            Ok(super::JobServerStyle::Fifo("/tmp/GMfifo6851")),
        );

        assert_eq!(
            super::parse_jobserver_auth(
                " -j2 --jobserver-auth=fifo:/tmp/GMfifo6852 --jobserver-auth=fifo:/tmp/GMfifo6851"
            ),
            Ok(super::JobServerStyle::Fifo("/tmp/GMfifo6851")),
        );
    }

    #[test]
    fn test_parse_jobserver_auth_pipe() {
        assert_eq!(
            super::parse_jobserver_auth("  -j3 --jobserver-auth=3,4 --jobserver-auth=-2,-2"),
            Ok(super::JobServerStyle::Pipe(-2, -2)),
        );

        assert_eq!(
            super::parse_jobserver_auth("  -j3 --jobserver-auth=3,4"),
            Ok(super::JobServerStyle::Pipe(3, 4)),
        );
    }
}
