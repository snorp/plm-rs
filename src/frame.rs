use std::convert::From;
use std::fmt;
use std::str::FromStr;

use bytes::{Buf, BufMut, BytesMut};

use bitflags::bitflags;

use nom::{self, alt, do_parse, named, number::streaming::be_u8, one_of, tag, take, take_until};
use tokio_util::codec::{Decoder, Encoder};

use crate::constants::*;
use crate::error::*;

/// An [Address] Represents an INSTEON device address. These are 3 bytes
/// and are commonly represented as hex numbers separated
/// by '.', e.g. '2b.a1.11'.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Address([u8; 3]);

impl From<[u8; 3]> for Address {
    fn from(b: [u8; 3]) -> Self {
        Address(b)
    }
}

impl<'a> From<&'a [u8]> for Address {
    fn from(b: &'a [u8]) -> Self {
        assert_eq!(b.len(), 3);

        let mut address = [0u8; 3];
        address.copy_from_slice(b);
        Address(address)
    }
}

impl FromStr for Address {
    type Err = Error;
    fn from_str(s: &str) -> std::result::Result<Self, <Self as FromStr>::Err> {
        let mut buf = [0u8; 3];

        let pieces: Vec<&str> = s.split('.').collect();
        for (idx, piece) in pieces.iter().enumerate() {
            let b = u8::from_str_radix(piece, 16);
            if b.is_err() {
                return Err(Error::InvalidAddress);
            }

            buf[idx] = b.unwrap();
        }

        Ok(Address(buf))
    }
}

impl From<Address> for [u8; 3] {
    fn from(address: Address) -> Self {
        address.0
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02x}.{:02x}.{:02x}", self.0[0], self.0[1], self.0[2])
    }
}

/// Represents the various link modes available.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AllLinkMode {
    /// In this mode, the modem is linked as a responder or receiver of events.
    Responder,
    /// In this mode, the modem is linked as a controller.
    Controller,
    /// In this mode, the effective link mode depends on the ordering in which
    /// the modem and device are entered into link mode.
    Auto,
    /// Causes a link to be deleted.
    Delete,
    /// When received in a [AllLinkComplete], indicates that no link was made.
    None,
}

impl fmt::Display for AllLinkMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<u8> for AllLinkMode {
    fn from(mode: u8) -> Self {
        match mode {
            LINK_MODE_RESPONDER => AllLinkMode::Responder,
            LINK_MODE_CONTROLLER => AllLinkMode::Controller,
            LINK_MODE_AUTO => AllLinkMode::Auto,
            LINK_MODE_DELETE => AllLinkMode::Delete,
            _ => AllLinkMode::None,
        }
    }
}

impl From<AllLinkMode> for u8 {
    fn from(mode: AllLinkMode) -> Self {
        match mode {
            AllLinkMode::Responder => LINK_MODE_RESPONDER,
            AllLinkMode::Controller => LINK_MODE_CONTROLLER,
            AllLinkMode::Auto => LINK_MODE_AUTO,
            AllLinkMode::Delete => LINK_MODE_DELETE,
            AllLinkMode::None => unimplemented!(),
        }
    }
}

bitflags! {
    /// Represents the link flags.
    pub struct AllLinkFlags: u8 {
        const IN_USE         = (1 << 7);
        /// When present, the modem is linked as a controller. If absent,
        /// the modem is a responder.
        const IS_CONTROLLER  = (1 << 6);
        const HAS_BEEN_USED  = (1 << 1);
        const NONE           = 0u8;
    }
}

bitflags! {
    /// Represents details about a [Message](super::Message).
    pub struct MessageFlags: u8 {
        /// When present along with the [MessageFlags::GROUP] flag below, this message is being
        /// broadcast to a group. The group number will be found in [cmd2](super::Message::cmd2).
        /// If [MessageFlags::GROUP] is not present, this represents an unacknowledged all-link message.
        const BROADCAST_OR_NAK = (1 << 7);
        /// The message is being broadcast to a group. Normally either
        /// [cmd1](super::Message::cmd1) or [cmd2](super::Message::cmd2) contains the group number.
        const GROUP            = (1 << 6);
        /// The message is an acknowledgement of a prior one.
        const ACK              = (1 << 5);
        /// The message is an extended message.
        const EXTENDED         = (1 << 4);
        const NONE             = 0;
    }
}

impl Default for MessageFlags {
    fn default() -> Self {
        MessageFlags::NONE
    }
}

/// Information about the attached modem.
#[derive(Debug, Clone, PartialEq)]
pub struct ModemInfo {
    /// The [Address] for the modem.
    pub address: Address,
    /// The device category for the modem.
    pub category: u8,
    /// The sub-category for the modem.
    pub sub_category: u8,
    /// The firmware version present in the modem.
    pub firmware_version: u8,
}

/// This represents a single link record in the modem's link database.
#[derive(Debug, Clone, PartialEq)]
pub struct AllLinkRecord {
    pub flags: AllLinkFlags,
    pub group: u8,
    pub to: Address,
    pub data: [u8; 3],
}

/// This represents the result of a completed link.
#[derive(Debug, Clone, PartialEq)]
pub struct AllLinkComplete {
    pub mode: AllLinkMode,
    pub group: u8,
    pub address: Address,
    pub category: u8,
    pub sub_category: u8,
    pub firmware_version: u8,
}

/// This represents a single command or response to and from the modem.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// Fetches the info for the current modem. The response will be in
    /// as `ModemInfo` frame.
    GetModemInfo,
    /// The response to `GetModemInfo`, containing the info for the current modem.
    ModemInfo(ModemInfo),
    /// Send a standard-length INSTEON message.
    StandardInsteonSend {
        /// The target of the message
        to: Address,
        /// The flags for the message
        flags: MessageFlags,
        /// The maximum number of hops for the message. 3 is normally sufficient.
        max_hops: u8,
        /// The value for cmd1
        cmd1: u8,
        /// The value for cmd2
        cmd2: u8,
    },
    /// Send an extended-length INSTEON message.
    ExtendedInsteonSend {
        /// The target of the message
        to: Address,
        /// The flags for the message
        flags: MessageFlags,
        /// The maximum number of hops for the message. 3 is normally sufficient.
        max_hops: u8,
        /// The value for cmd1
        cmd1: u8,
        /// The value for cmd1
        cmd2: u8,
        /// The extended data, which is device-specific.
        data: [u8; 14],
    },
    /// Produced when a standard INSTEON message is received.
    StandardInsteonReceive {
        /// The `Address` of the device that sent the message.
        from: Address,
        /// The `Address` of the intended recipient of the message.
        to: Address,
        /// The flags for the message
        flags: MessageFlags,
        /// The number of hops remaining when the message was received.
        /// e.g., if this is 2, and max_hops is 3, the message was received
        /// directly without any relay.
        hops_remaining: u8,
        /// The maximum number of hops allowed for this message.
        max_hops: u8,
        /// The value for cmd1
        cmd1: u8,
        /// The value for cmd2
        cmd2: u8,
    },
    /// Produced when an extended INSTEON message is received.
    ExtendedInsteonReceive {
        from: Address,
        to: Address,
        flags: MessageFlags,
        hops_remaining: u8,
        max_hops: u8,
        cmd1: u8,
        cmd2: u8,
        data: [u8; 14],
    },
    /// Puts the modem into linking mode
    StartAllLink {
        mode: AllLinkMode,
        group: u8,
    },
    /// Exits linking mode
    CancelAllLink,
    AllLinkComplete(AllLinkComplete),
    GetFirstAllLinkRecord,
    GetNextAllLinkRecord,
    AllLinkRecord(AllLinkRecord),
    Reset,
    AllLinkCommand {
        group: u8,
        cmd1: u8,
        cmd2: u8,
    },
    Unknown {
        buf: Vec<u8>,
    },
}

fn clone_from_slice<A, T>(slice: &[T]) -> A
where
    A: Default + AsMut<[T]>,
    T: Clone,
{
    let mut a = Default::default();
    <A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
    a
}

impl Frame {
    /// Returns true if `other` is a response to self.
    pub fn is_response(&self, other: &Frame) -> bool {
        match (self, other) {
            (Frame::GetModemInfo, Frame::ModemInfo { .. }) => true,
            _ => ::std::mem::discriminant(self) == ::std::mem::discriminant(other),
        }
    }

    pub fn from_slice(src: &[u8]) -> Result<Option<Frame>, Error> {
        let mut bytes = BytesMut::new();
        bytes.extend_from_slice(src);
        Self::from_bytes(&mut bytes)
    }

    /// Parse a frame. Returns the number of bytes consumed along with
    /// a `Frame` or `Error`. Note that some errors, such as
    /// `Error::NotAcknowledged`, can consume bytes.
    ///
    /// # Arguments
    /// * `src` - The buffer to parse.
    pub fn from_bytes(src: &mut BytesMut) -> Result<Option<Frame>, Error> {
        const TERMS: [u8; 2] = [ACK, NAK];

        #[rustfmt::skip]
        named!(parse_frame<(u8, Frame)>,
            alt!(
                // Sometimes we get a spurious ACK, so take care of that.
                do_parse!(
                    tag!(&[ACK][..])               >>
                    buf: take_until!(&[START][..]) >>
                    (ACK, Frame::Unknown{
                        buf: buf.to_vec()
                    })
                ) |
                // ModemInfo
                do_parse!(
                    tag!(&[START, GETIMINFO][..])  >>
                    address: take!(3)              >>
                    category: be_u8                >>
                    sub_category: be_u8            >>
                    firmware_version: be_u8        >>
                    ack: one_of!(TERMS)            >>
                    (ack as u8, Frame::ModemInfo(ModemInfo {
                        address: address.into(),
                        category, sub_category, firmware_version
                    }))
                ) |
                // StandardInsteonReceive
                do_parse!(
                    tag!(&[START, STANDARD_INSTEON_RECV][..]) >>
                    from: take!(3)                            >>
                    to: take!(3)                              >>
                    flags: be_u8                              >>
                    cmd1: be_u8                               >>
                    cmd2: be_u8                               >>
                    (ACK, Frame::StandardInsteonReceive {
                        from: from.into(),
                        to: to.into(),
                        flags: MessageFlags::from_bits_truncate(flags),
                        hops_remaining: (flags & 0b1100) >> 2,
                        max_hops: flags & 0b11,
                        cmd1, cmd2
                    })
                ) |
                // ExtendedInsteonReceive
                do_parse!(
                    tag!(&[START, EXTENDED_INSTEON_RECV][..]) >>
                    from: take!(3)                            >>
                    to: take!(3)                              >>
                    flags: be_u8                              >>
                    cmd1: be_u8                               >>
                    cmd2: be_u8                               >>
                    data: take!(14)                           >>
                    (ACK, Frame::ExtendedInsteonReceive {
                        from: from.into(),
                        to: to.into(),
                        flags: MessageFlags::from_bits_truncate(flags),
                        hops_remaining: (flags & 0b1100) >> 2,
                        max_hops: flags & 0b11,
                        cmd1, cmd2, data: clone_from_slice(data)
                    })
                ) |
                // StandardInsteonSend
                do_parse!(
                    tag!(&[START, INSTEON_SEND][..]) >>
                    to: take!(3)                     >>
                    flags: be_u8                     >>
                    cmd1: be_u8                      >>
                    cmd2: be_u8                      >>
                    ack: one_of!(TERMS)              >>
                    (ack as u8, Frame::StandardInsteonSend {
                        to: to.into(),
                        flags: MessageFlags::from_bits_truncate(flags),
                        max_hops: flags & 0b11,
                        cmd1, cmd2
                    })
                ) |
                // ExtendedInsteonSend
                do_parse!(
                    tag!(&[START, INSTEON_SEND][..]) >>
                    to: take!(3)                     >>
                    flags: be_u8                     >>
                    cmd1: be_u8                      >>
                    cmd2: be_u8                      >>
                    data: take!(14)                  >>
                    ack: one_of!(TERMS)              >>
                    (ack as u8, Frame::ExtendedInsteonSend {
                        to: to.into(),
                        flags: MessageFlags::from_bits_truncate(flags),
                        max_hops: flags & 0b11,
                        cmd1, cmd2, data: clone_from_slice(data)
                    })
                ) |
                // StartAllLink
                do_parse!(
                    tag!(&[START, START_ALL_LINK][..]) >>
                    mode: be_u8                        >>
                    group: be_u8                       >>
                    ack: one_of!(TERMS)                >>
                    (ack as u8, Frame::StartAllLink {
                        mode: mode.into(), group
                    })
                ) |
                // CancelAllLink
                do_parse!(
                    tag!(&[START, CANCEL_ALL_LINK][..])  >>
                    ack: one_of!(TERMS)                  >>
                    (ack as u8, Frame::CancelAllLink)
                ) |
                // AllLinkComplete
                do_parse!(
                    tag!(&[START, ALL_LINK_COMPLETE][..])  >>
                    mode: be_u8                            >>
                    group: be_u8                           >>
                    from: take!(3)                         >>
                    category: be_u8                        >>
                    sub_category: be_u8                    >>
                    firmware_version: be_u8                >>
                    (ACK, Frame::AllLinkComplete(AllLinkComplete{
                        mode: mode.into(),
                        group,
                        address: from.into(),
                        category, sub_category, firmware_version
                    }))
                ) |
                // GetFirstAllLinkRecord
                do_parse!(
                    tag!(&[START, GET_FIRST_ALL_LINK_RECORD][..])  >>
                    ack: one_of!(TERMS)                            >>
                    (ack as u8, Frame::GetFirstAllLinkRecord)
                ) |
                // GetNextAllLinkRecord
                do_parse!(
                    tag!(&[START, GET_NEXT_ALL_LINK_RECORD][..])  >>
                    ack: one_of!(TERMS)                           >>
                    (ack as u8, Frame::GetNextAllLinkRecord)
                ) |
                // AllLinkRecord
                do_parse!(
                    tag!(&[START, ALL_LINK_RECORD][..])  >>
                    flags: be_u8                         >>
                    group: be_u8                         >>
                    to: take!(3)                         >>
                    data: take!(3)                       >>
                    (ACK, Frame::AllLinkRecord(AllLinkRecord {
                        flags: AllLinkFlags::from_bits_truncate(flags),
                        group,
                        to: to.into(),
                        data: [data[0], data[1], data[2]]
                    }))
                ) |
                // Reset
                do_parse!(
                    tag!(&[START, RESET][..])  >>
                    ack: one_of!(TERMS)        >>
                    (ack as u8, Frame::Reset)
                ) |
                // AllLinkCommand
                do_parse!(
                    tag!(&[START, ALL_LINK_SEND][..]) >>
                    group: be_u8                      >>
                    cmd1: be_u8                       >>
                    cmd2: be_u8                       >>
                    ack: one_of!(TERMS)               >>
                    (ack as u8, Frame::AllLinkCommand {
                        group, cmd1, cmd2
                    })
                )
            )
        );

        match parse_frame(src) {
            Ok((remainder, (ack, frame))) => {
                let consumed = src.len() - remainder.len();
                src.advance(consumed);
                if ack != ACK {
                    Err(Error::NotAcknowledged)
                } else {
                    Ok(Some(frame))
                }
            }
            Err(nom::Err::Incomplete(_)) => Ok(None),
            Err(nom::Err::Error((_, nom::error::ErrorKind::Alt))) => Err(Error::Parse),
            Err(nom::Err::Error((_, kind))) => Err(kind.into()),
            Err(nom::Err::Failure((_, kind))) => Err(kind.into()),
        }
    }

    /// Serializes the `Frame` into the returned `Vec<u8>`.
    pub fn to_bytes(&self, bytes: &mut BytesMut) {
        bytes.put_u8(START);
        match *self {
            Frame::GetModemInfo { .. } => bytes.put_u8(GETIMINFO),
            Frame::StandardInsteonSend {
                ref to,
                ref flags,
                ref max_hops,
                ref cmd1,
                ref cmd2,
            } => {
                bytes.put_u8(INSTEON_SEND);
                bytes.put_slice(&to.0);

                let mut flags = (*flags).bits();
                flags |= (max_hops & 0b11) << 2;
                flags |= max_hops & 0b11;
                bytes.put_u8(flags);

                bytes.put_u8(*cmd1);
                bytes.put_u8(*cmd2);
            }
            Frame::ExtendedInsteonSend {
                ref to,
                ref flags,
                ref max_hops,
                ref cmd1,
                ref cmd2,
                ref data,
            } => {
                bytes.put_u8(INSTEON_SEND);
                bytes.put_slice(&to.0);

                let mut flags = (*flags).bits();
                flags |= (max_hops & 0b11) << 2;
                flags |= max_hops & 0b11;
                bytes.put_u8(flags);

                bytes.put_u8(*cmd1);
                bytes.put_u8(*cmd2);
                bytes.put_slice(&data[..]);

                // We need to calculate a checksum and stick it in the last data slot.
                // This is the two's complement of the sum of all bytes between
                // cmd1 and the end of the buffer, inclusive.
                let sum = bytes[6..].iter().fold(0u32, |sum, x| sum + u32::from(*x));
                *(bytes.last_mut().unwrap()) = ((!sum + 1) & 255) as u8;
            }
            Frame::StartAllLink {
                ref mode,
                ref group,
            } => {
                bytes.put_u8(START_ALL_LINK);
                bytes.put_u8((*mode).into());
                bytes.put_u8(*group);
            }
            Frame::CancelAllLink => bytes.put_u8(CANCEL_ALL_LINK),
            Frame::GetFirstAllLinkRecord => bytes.put_u8(GET_FIRST_ALL_LINK_RECORD),
            Frame::GetNextAllLinkRecord => bytes.put_u8(GET_NEXT_ALL_LINK_RECORD),
            Frame::Reset => bytes.put_u8(RESET),
            Frame::AllLinkCommand {
                ref group,
                ref cmd1,
                ref cmd2,
            } => {
                bytes.put_u8(ALL_LINK_SEND);
                bytes.put_u8(*group);
                bytes.put_u8(*cmd1);
                bytes.put_u8(*cmd2);
            }
            _ => unimplemented!(),
        }
    }
}

pub struct FrameCodec();

impl Decoder for FrameCodec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match Frame::from_bytes(src) {
            Ok(val) => Ok(val),
            Err(e) => Err(e),
        }
    }
}

impl Encoder<Frame> for FrameCodec {
    type Error = Error;
    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.to_bytes(dst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_parse() {
        assert_eq!(
            Address([0x11, 0x22, 0x33]),
            Address::from_str("11.22.33").unwrap()
        );
    }

    #[test]
    fn address_parse_no_dots() {
        assert_eq!(Err(Error::InvalidAddress), Address::from_str("112233"));
    }

    #[test]
    fn no_command() {
        let buf = &[START][..];
        assert_eq!(Frame::from_slice(buf), Ok(None));
    }

    #[test]
    fn no_terminator() {
        let buf = &[START, GETIMINFO][..];
        assert_eq!(Frame::from_slice(&buf), Ok(None));
    }

    #[test]
    fn unknown_command() {
        let buf = &[START, 0x95u8][..];
        assert_eq!(Frame::from_slice(&buf), Err(Error::Parse));
    }

    #[test]
    fn garbage() {
        let buf = &[0x1u8; 128][..];
        assert_eq!(Frame::from_slice(&buf), Err(Error::Parse));
    }

    #[test]
    fn valid() {
        let buf = &[START, CANCEL_ALL_LINK, ACK][..];
        assert_eq!(Frame::from_slice(&buf), Ok(Some(Frame::CancelAllLink)));
    }
}
