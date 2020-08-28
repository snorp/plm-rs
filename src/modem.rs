use std::convert::TryFrom;
use std::io;
use std::path::Path;
use std::time::Duration;

use log::{debug, error, warn};

use futures::{
    future::FutureExt,
    select_biased,
    stream::{Stream, StreamExt},
};

use futures_timer::Delay;

use crate::broker::*;
use crate::error::*;
use crate::frame::*;
use crate::message::*;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

const NUM_RETRIES: u8 = 20;
const RETRY_DELAY: Duration = Duration::from_millis(250);

/// The default duration to wait for [Message] replies. 10 seconds.
pub const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(10);


/// A [Modem] is a connection to an INSTEON Modem. It can be used to send
/// [Message]s and manage device links (e.g. [Modem::link_device]).
pub struct Modem {
    broker: Broker,
}

impl Modem {
    /// Constructs a new `Modem` given a path to a serial port
    ///
    /// # Arguments
    /// * `path` - The path to a serial port with an INSTEON modem attached.
    pub fn from_path(path: impl AsRef<Path> + Send + 'static) -> io::Result<Self> {
        debug!("Creating Modem with path {}", path.as_ref().display());

        let broker = Broker::from_path(path)?;

        Ok(Self { broker })
    }

    /// Constructs a new `Modem` from an arbitrary I/O modem
    ///
    /// # Arguments
    /// * `handle` - An async readable, writable modem
    pub fn new(handle: impl AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static) -> Modem {
        Self {
            broker: Broker::new(handle),
        }
    }

    async fn send_frame(&mut self, frame: Frame) -> Result<Frame, Error> {
        let mut retries = NUM_RETRIES;
        loop {
            retries -= 1;
            debug!(
                "Sending Frame (attempt {}) {:02x?}",
                NUM_RETRIES - retries,
                frame
            );

            match self.broker.send(frame.clone()).await {
                Ok(response) => {
                    debug!("Received Response: {:02x?}", response);
                    return Ok(response);
                }
                Err(Error::NotAcknowledged) if retries > 0 => {
                    warn!("Frame not acknowledged, retrying after {:?}", RETRY_DELAY);
                    Delay::new(RETRY_DELAY).await;
                    continue;
                }
                e => {
                    error!("Failed to send frame, {:02x?}", e);
                    return e;
                }
            }
        }
    }

    async fn send_message_direct(&mut self, message: Message) -> Result<Message, Error> {
        debug!("Sending Message {:02x?}", message);

        let mut listener = self.listen().await?;

        if message.flags.contains(MessageFlags::EXTENDED) {
            self.send_frame(Frame::ExtendedInsteonSend {
                to: message.to,
                flags: message.flags,
                max_hops: message.max_hops,
                cmd1: message.cmd1.into(),
                cmd2: message.cmd2.into(),
                data: message.data,
            })
            .await?;
        } else {
            self.send_frame(Frame::StandardInsteonSend {
                to: message.to,
                flags: message.flags,
                max_hops: message.max_hops,
                cmd1: message.cmd1.into(),
                cmd2: message.cmd2.into(),
            })
            .await?;
        }

        while let Some(response) = listener.next().await {
            debug!("Received Message: {:02x?}", response);
            if message.is_ack(&response) {
                return Ok(response);
            }
        }

        Ok(message)
    }

    /// Sends a [Message]. This uses the default timeout
    /// duration defined by [DEFAULT_TIMEOUT_DURATION].
    ///
    /// Returns an acknowledged [Message] or an error.
    pub async fn send_message(&mut self, message: Message) -> Result<Message, Error> {
        self.send_message_with_timeout(message, DEFAULT_TIMEOUT_DURATION)
            .await
    }

    /// Sends a [Message] with the specified timeout duration.
    ///
    /// Returns an acknowledged [Message] or an error.
    pub async fn send_message_with_timeout(
        &mut self,
        message: Message,
        duration: Duration,
    ) -> Result<Message, Error> {
        let mut delay = Delay::new(duration).fuse();
        let mut sending = Box::pin(self.send_message_direct(message).fuse());

        select_biased! {
            e = delay => Err(Error::Timeout),
            r = sending => r
        }
    }

    /// Retrieve information about the attached modem.
    pub async fn get_info(&mut self) -> Result<ModemInfo, Error> {
        match self.send_frame(Frame::GetModemInfo).await? {
            Frame::ModemInfo(info) => Ok(info),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Return the link database stored in the modem.
    pub async fn get_links(&mut self) -> Result<impl Iterator<Item = AllLinkRecord>, Error> {
        let mut records = Vec::new();
        let mut listener = self.listen_frames().await?;

        self.send_frame(Frame::GetFirstAllLinkRecord).await?;

        while let Some(frame) = listener.next().await {
            match frame {
                Frame::AllLinkRecord(record) => {
                    debug!("Got All Link {:?}", record);
                    records.push(record);
                    if let Err(Error::NotAcknowledged) =
                        self.broker.send(Frame::GetNextAllLinkRecord).await
                    {
                        // There's no more
                        break;
                    }
                }
                _ => return Err(Error::UnexpectedResponse),
            }
        }

        Ok(records.into_iter())
    }

    async fn listen_frames(
        &mut self,
    ) -> Result<impl Stream<Item = Frame> + Sync + Send + Unpin, Error> {
        self.broker.listen().await
    }

    /// Listens for incoming [Message]s and delivers them on the returned [Stream].
    pub async fn listen(
        &mut self,
    ) -> Result<impl Stream<Item = Message> + Sync + Send + Unpin, Error> {
        Ok(Box::pin(self.broker.listen().await?.filter_map(
            |frame| async {
                if let Ok(message) = Message::try_from(frame) {
                    Some(message)
                } else {
                    None
                }
            },
        )))
    }

    /// Link a new device to the modem.
    pub async fn link_device(
        &mut self,
        address: Option<Address>,
        mode: AllLinkMode,
        group: u8,
    ) -> Result<AllLinkComplete, Error> {
        // Ensure we're not in some prior linking mode
        self.send_frame(Frame::CancelAllLink).await?;

        // We need to listen for some frames
        let mut listener = self.listen_frames().await?;

        // If we have an address, ask the device to enter linking mode
        if let Some(address) = address {
            self.send_message(
                (
                    address,
                    Command::StartLinking,
                    Command::from(group),
                    MessageFlags::EXTENDED,
                )
                    .into(),
            )
            .await?;
        }

        // Put modem into linking mode first.
        self.send_frame(Frame::StartAllLink { mode, group }).await?;

        // Wait for an AllLinkComplete record
        let mut result = Err(Error::UnexpectedResponse);
        while let Some(frame) = listener.next().await {
            match frame {
                Frame::AllLinkComplete(info) => {
                    result = Ok(info);
                    break;
                }
                _ => continue,
            }
        }

        // We don't need to listen anymore
        drop(listener);

        // Again, if we have a device, ask it to exit linking mode
        if let Some(address) = address {
            let _ = self
                .send_message(
                    (
                        address,
                        Command::CancelLinking,
                        Command::from(group),
                        MessageFlags::EXTENDED,
                    )
                        .into(),
                )
                .await; // We don't really care if it worked or not
        }

        // Ensure we exit linking mode
        let _ = self.send_frame(Frame::CancelAllLink).await;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;
    use std::{
        env,
        sync::{Arc, Mutex},
    };

    const MODEM_ENV_VAR: &str = "MODEM_PATH";

    lazy_static! {
        static ref MODEM: Arc<Mutex<Modem>> = {
            pretty_env_logger::init();

            Arc::new(Mutex::new(
                Modem::from_path(env::var(MODEM_ENV_VAR).unwrap()).unwrap(),
            ))
        };
    }

    macro_rules! assume_modem {
        () => {
            if env::var(MODEM_ENV_VAR).is_err() {
                return ();
            }
        };
    }

    #[async_std::test]
    async fn get_info() {
        assume_modem!();

        let info = MODEM.lock().unwrap().get_info().await.unwrap();
        assert_eq!(info.category, 3);
    }

    #[async_std::test]
    async fn get_links() {
        assume_modem!();

        let links: Vec<AllLinkRecord> = MODEM.lock().unwrap().get_links().await.unwrap().collect();
        assert!(!links.is_empty());
    }

    #[test]
    fn bad_path() {
        assert!(Modem::from_path("/this/does/not/exist").is_err());
    }
}
