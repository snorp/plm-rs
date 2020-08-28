use std::path::Path;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    future::FutureExt,
    select,
    sink::SinkExt,
    stream::{Stream, StreamExt},
};

use log::debug;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{DataBits, FlowControl, Parity, Serial, SerialPortSettings, StopBits};
use tokio_util::codec::*;

use crate::error::*;
use crate::frame::*;

pub enum BrokerMessage {
    AddListener {
        listener: UnboundedSender<Frame>,
    },
    SendFrame {
        frame: Frame,
        responder: UnboundedSender<Result<Frame, Error>>,
    },
}

pub struct Broker {
    sender: UnboundedSender<BrokerMessage>,
}

async fn event_loop(
    mut receiver: UnboundedReceiver<BrokerMessage>,
    mut framed: Framed<impl AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static, FrameCodec>,
) {
    let mut listeners = Vec::<UnboundedSender<Frame>>::new();

    loop {
        select! {
            maybe_frame = framed.next().fuse() => match(maybe_frame) {
                Some(Ok(frame)) => {
                    debug!("Received Frame: {:02x?}", frame);

                    let mut new_listeners = Vec::with_capacity(listeners.len());
                    while let Some(mut listener) = listeners.pop() {
                        if listener.send(frame.clone()).await.is_ok() {
                            new_listeners.push(listener);
                        }
                    }

                    listeners = new_listeners;
                },
                _ => break,
            },
            msg = receiver.next() => {
                match (msg) {
                    Some(BrokerMessage::AddListener{ listener }) => {
                        listeners.push(listener);
                    },
                    Some(BrokerMessage::SendFrame{ frame, mut responder }) => {
                        debug!("Sending Frame: {:02x?}", frame);
                        if let Err(e) = framed.send(frame).await {
                            let _ = responder.send(Err(e)).await;
                            continue;
                        }

                        match framed.next().await {
                            None => {
                                let _ = responder.send(Err(Error::Disconnected)).await;
                                break;
                            },
                            Some(response) => {
                                debug!("Received Response: {:02x?}", response);
                                let _ = responder.send(response).await;
                            }
                        }
                    },
                    None => break, // No more messages coming, exit
                }
            }
        }
    }
}

impl Broker {
    pub fn from_path(path: impl AsRef<Path> + Send + 'static) -> Result<Broker, std::io::Error> {
        let (sender, receiver) = unbounded();

        let (init_sender, init_receiver) = channel();

        thread::spawn(move || {
            let mut rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let settings = SerialPortSettings {
                    baud_rate: 19200,
                    data_bits: DataBits::Eight,
                    flow_control: FlowControl::None,
                    parity: Parity::None,
                    stop_bits: StopBits::One,
                    timeout: Duration::from_millis(100),
                };

                match Serial::from_path(path.as_ref(), &settings) {
                    Ok(port) => {
                        init_sender.send(Ok(())).unwrap();
                        event_loop(receiver, Framed::new(port, FrameCodec())).await
                    }
                    Err(e) => init_sender.send(Err(e)).unwrap(),
                }
            });
        });

        // Make sure we were able to create the port
        init_receiver.recv().unwrap()?;
        Ok(Broker { sender })
    }

    pub fn new(handle: impl AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static) -> Broker {
        let (sender, receiver) = unbounded();

        thread::spawn(move || {
            let mut rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(
                async move { event_loop(receiver, Framed::new(handle, FrameCodec())).await },
            );
        });

        Broker { sender }
    }

    pub async fn send(&mut self, frame: Frame) -> Result<Frame, Error> {
        let (sender, mut receiver) = unbounded();
        self.sender
            .send(BrokerMessage::SendFrame {
                frame,
                responder: sender,
            })
            .await?;
        receiver.next().await.ok_or_else(|| Error::Disconnected)?
    }

    pub async fn listen(&mut self) -> Result<impl Stream<Item = Frame>, Error> {
        let (sender, receiver) = unbounded();
        self.sender
            .send(BrokerMessage::AddListener { listener: sender })
            .await?;
        Ok(receiver)
    }
}
