use std::{convert::TryFrom, fmt};

use crate::error::*;
use crate::frame::*;

/// A [Command] (two, actually) is sent in a [Message].
/// This type has some commonly used ones, but you can send
/// arbitrary values via [Command::Other].
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Command {
    /// When sent to a device, turns the device on.
    /// When received, it indicates that the device was turned on by manipulation.
    On,

    /// When sent to a device, turns the device on faster, e.g. no ramping.
    /// When received, it indicates that the device performed a "fast on",
    /// usually by a double-tapped switch.
    OnFast,

    /// When sent to a device, turns the device off.
    /// When received, it indicates that the device was turned off by manipulation.
    Off,

    /// When sent to a device, turns the device off faster, e.g. no ramping.
    /// When received, it indicates that the device performed a "fast off",
    /// usually by a double-tapped switch.
    OffFast,

    /// Ping the device.
    Ping,

    /// Retrieves the protocol version information.
    VersionQuery,

    /// Cancels linking mode for the device.
    CancelLinking,

    /// Starts linking mode for the device.
    StartLinking,

    /// Queries the status of the device.
    StatusRequest,

    /// Causes the device to beep once.
    Beep,

    /// Arbitrary commands not covered by one of the cases above.
    Other(u8),

    None,
}

impl Default for Command {
    fn default() -> Self {
        Command::None
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<u8> for Command {
    fn from(b: u8) -> Self {
        use Command::*;
        match b {
            0x08u8 => CancelLinking,
            0x09u8 => StartLinking,
            0x0du8 => VersionQuery,
            0x0fu8 => Ping,
            0x19u8 => StatusRequest,
            0x11u8 => On,
            0x12u8 => OnFast,
            0x13u8 => Off,
            0x14u8 => OffFast,
            0x30u8 => Beep,
            0 => None,
            _ => Other(b),
        }
    }
}

impl From<Command> for u8 {
    fn from(c: Command) -> Self {
        use Command::*;
        match c {
            On => 0x11u8,
            OnFast => 0x12u8,
            Off => 0x13u8,
            OffFast => 0x14u8,
            Ping => 0x0fu8,
            VersionQuery => 0x0du8,
            CancelLinking => 0x08u8,
            StartLinking => 0x09u8,
            StatusRequest => 0x19u8,
            Beep => 0x30u8,
            Other(cmd) => cmd,
            None => 0u8,
        }
    }
}

/// A [Message] can be sent to a device with a given [Address].
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Message {
    /// The address of the device that sent the `Message`.
    pub from: Address,

    /// The address of the receipient of the `Message`.
    pub to: Address,

    /// Flags describing various attributes of the `Message`.
    pub flags: MessageFlags,

    /// The number of hops remaining
    pub hops_remaining: u8,

    /// The maximum number of hops allowed for this `Message`.
    pub max_hops: u8,

    /// The first `Command` contained in the `Message`.
    pub cmd1: Command,

    /// The second `Command` contained in the `Message`. Often this is a group number or other
    /// details accompanying `cmd1`.
    pub cmd2: Command,

    /// Arbitrary user data, only available in an extended `Message`.
    pub data: [u8; 14],
}

impl Message {
    /// Returns true if `other` is an ACK of `self`.
    pub fn is_ack(&self, other: &Message) -> bool {
        match *other {
            Message { from, flags, .. } if self.to == from && flags.contains(MessageFlags::ACK) => {
                true
            }
            _ => false,
        }
    }
}

impl Default for Message {
    fn default() -> Self {
        Message {
            from: Address::default(),
            to: Address::default(),
            flags: MessageFlags::default(),
            hops_remaining: 3,
            max_hops: 3,
            cmd1: Command::default(),
            cmd2: Command::default(),
            data: [0u8; 14],
        }
    }
}

impl From<(Address, Command)> for Message {
    fn from(c: (Address, Command)) -> Self {
        let mut msg = Message::default();
        let (to, cmd1) = c;
        msg.to = to;
        msg.cmd1 = cmd1;
        msg
    }
}

impl From<(Address, Command, Command)> for Message {
    fn from(c: (Address, Command, Command)) -> Self {
        let mut msg = Message::default();
        let (to, cmd1, cmd2) = c;
        msg.to = to;
        msg.cmd1 = cmd1;
        msg.cmd2 = cmd2;
        msg
    }
}

impl From<(Address, Command, MessageFlags)> for Message {
    fn from(c: (Address, Command, MessageFlags)) -> Self {
        let mut msg = Message::default();
        let (to, cmd1, flags) = c;
        msg.to = to;
        msg.cmd1 = cmd1;
        msg.flags = flags;
        msg
    }
}

impl From<(Address, Command, Command, MessageFlags)> for Message {
    fn from(c: (Address, Command, Command, MessageFlags)) -> Self {
        let mut msg = Message::default();
        let (to, cmd1, cmd2, flags) = c;
        msg.to = to;
        msg.cmd1 = cmd1;
        msg.cmd2 = cmd2;
        msg.flags = flags;
        msg
    }
}

impl TryFrom<Frame> for Message {
    type Error = Error;

    fn try_from(frame: Frame) -> Result<Self, Self::Error> {
        match frame {
            Frame::StandardInsteonReceive {
                from,
                to,
                flags,
                hops_remaining,
                max_hops,
                cmd1,
                cmd2,
            } => Ok(Message {
                from,
                to,
                flags,
                hops_remaining,
                max_hops,
                cmd1: cmd1.into(),
                cmd2: cmd2.into(),
                data: [0u8; 14],
            }),
            Frame::ExtendedInsteonReceive {
                from,
                to,
                flags,
                hops_remaining,
                max_hops,
                cmd1,
                cmd2,
                data,
            } => Ok(Message {
                from,
                to,
                flags,
                hops_remaining,
                max_hops,
                cmd1: cmd1.into(),
                cmd2: cmd2.into(),
                data,
            }),
            _ => Err(Error::UnexpectedResponse),
        }
    }
}
