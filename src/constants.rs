pub const START: u8 = 0x02u8;
pub const ACK: u8 = 0x06u8;
pub const NAK: u8 = 0x15u8;

// PLM -> Host commands
pub const STANDARD_INSTEON_RECV: u8 = 0x50u8;
pub const EXTENDED_INSTEON_RECV: u8 = 0x51u8;
pub const ALL_LINK_COMPLETE: u8 = 0x53u8;
pub const ALL_LINK_RECORD: u8 = 0x57u8;
pub const GETIMINFO: u8 = 0x60u8;

// Host -> PLM commands
pub const ALL_LINK_SEND: u8 = 0x61u8;
pub const INSTEON_SEND: u8 = 0x62u8;
pub const START_ALL_LINK: u8 = 0x64u8;
pub const CANCEL_ALL_LINK: u8 = 0x65u8;
pub const RESET: u8 = 0x67u8;
pub const GET_FIRST_ALL_LINK_RECORD: u8 = 0x69u8;
pub const GET_NEXT_ALL_LINK_RECORD: u8 = 0x6au8;

// Linking modes
pub const LINK_MODE_RESPONDER: u8 = 0x00;
pub const LINK_MODE_CONTROLLER: u8 = 0x01;
pub const LINK_MODE_AUTO: u8 = 0x03;
pub const LINK_MODE_DELETE: u8 = 0xff;
